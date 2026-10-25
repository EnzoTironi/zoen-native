//! Signed remote ordinary pages must not repeatedly fold their entire prefix.
use super::*;
use crate::sync::replay_probe::{page_body, projection, Home};

const SPACE: &str = "sp_remote_page_catchup";

fn pages(
    author: &Author,
    count: usize,
) -> std::result::Result<Vec<Sequenced>, Box<dyn std::error::Error>> {
    let mut head = None;
    let mut events = vec![sequence(
        Envelope::plain(&author.sign_event(
            SPACE,
            "create",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Remote pages".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )?)?,
        &mut head,
    )?];
    for index in 0..count {
        events.push(sequence(
            Envelope::plain(&author.sign_event(
                SPACE,
                &format!("remote-page-{index}"),
                2 + index as i64,
                head.clone(),
                page_body(index),
            )?)?,
            &mut head,
        )?);
    }
    Ok(events)
}

#[tokio::test(flavor = "current_thread")]
async fn ordinary_remote_page_catchup_is_single_pass_and_cold_projection_exact(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let home = Home(std::env::temp_dir().join(types::new_id("catchup-projection").unwrap()));
    std::fs::create_dir(&home.0).unwrap();
    let path = home.0.join("reader.sqlite");
    let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
    engine.create_account("Reader", "reader", &url).unwrap();
    engine.set_registered(true).unwrap();
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    let author = Author::device(&Signer::generate().unwrap(), Signer::generate().unwrap());
    let events = pages(&author, 32)?;
    let engine = Arc::new(Mutex::new(engine));
    let (status, _) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    let ctx = Ctx {
        engine: EngineLease::new(engine.clone()),
        listener: None,
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: None,
    };
    let (cmd, mut commands) = mpsc::unbounded_channel();
    let (finished, stopped) = oneshot::channel();
    let relay = async {
        let mut socket = catch_up_socket(&listener).await;
        crate::sync::replay_probe::reset();
        for event in &events {
            reply(&mut socket, ServerFrame::Event { ev: event.clone() }).await;
        }
        reply(&mut socket, ServerFrame::SyncDone).await;
        // This request is processed after the earlier SyncDone on the same stream.
        // No sleep or observed timeout substitutes for the ordered barrier.
        let (answer, answered) = oneshot::channel();
        cmd.send(Cmd::Req {
            op: Op::FetchLink {
                id: "projection-barrier".into(),
            },
            reply: answer,
        })
        .unwrap();
        loop {
            match request(&mut socket).await {
                ClientFrame::Req {
                    id,
                    op: Op::FetchLink { .. },
                } => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Link(None)),
                        },
                    )
                    .await;
                    break;
                }
                ClientFrame::Ping | ClientFrame::Ephemeral { .. } => {}
                other => panic!("unexpected catch-up traffic: {other:?}"),
            }
        }
        assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
        assert!(ctx.status.borrow().synced);
        let folds = crate::sync::replay_probe::steps();
        eprintln!("page catch-up work: remote_pages=32 full_replay_steps={folds}");
        cmd.send(Cmd::Stop).unwrap();
        // Keep this owned socket alive until the client has consumed Stop. An
        // EOF racing that command would test fixture teardown, not catch-up.
        stopped.await.unwrap();
        folds
    };
    let mut backoff = Duration::from_secs(2);
    let (exit, folds) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            async {
                let exit = session(&ctx, &mut commands, &mut backoff).await;
                finished.send(()).unwrap();
                exit
            },
            relay
        )
    })
    .await
    .expect("bounded synthetic catch-up");
    assert!(matches!(exit, Exit::Stop));
    let projected = projection(&lock(&engine));
    assert_eq!(projected.len(), 32);
    for index in 0..32 {
        assert_eq!(
            lock(&engine)
                .page_markdown(&format!("remote-item-{index}"))
                .unwrap(),
            format!("# Page {index}\n\nUnicode 😀 and **unchanged** source.\n")
        );
    }
    let cursor = lock(&engine).next_seq(SPACE);
    assert_eq!(cursor, 33);
    assert_eq!(lock(&engine).store.outbox_len().unwrap(), 0);
    ctx.engine.revoke();
    drop(ctx);
    drop(engine);
    let reopened = Engine::open(path.to_str().unwrap()).unwrap();
    assert_eq!(projection(&reopened), projected);
    assert_eq!(reopened.next_seq(SPACE), cursor);
    assert!(reopened.logs[SPACE].verify().is_ok());
    assert_eq!(folds, 0, "ordinary empty-overlay catch-up must apply each new event once, not replay its whole prefix");
    Ok(())
}
