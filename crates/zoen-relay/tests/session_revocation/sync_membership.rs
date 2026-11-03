//! Actual WS/directory, synthetic atomic history seam; FDB has its own contract.
use super::*;
use types::{EventBody, Seen};
use zoen_relay::log::VisiblePage;
const SPACE: &str = "sp_page_removal";
const PAGE: usize = 500; // The unchanged production Sync page boundary.

pub(super) struct History {
    reader: String,
    rows: std::sync::Mutex<(Vec<Sequenced>, Option<u64>)>,
    paused: AtomicBool,
    entered: Notify,
    release: Semaphore,
}
impl History {
    fn push(
        rows: &mut Vec<Sequenced>,
        owner: &Author,
        body: EventBody,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let seq = rows.len() as u64;
        let prev = rows
            .last()
            .map_or(types::GENESIS_PREV, |ev| ev.hash.as_str())
            .to_string();
        let seen = rows.last().map(|ev| Seen {
            seq: ev.seq,
            hash: ev.hash.clone(),
        });
        let event = owner.sign_event(
            SPACE,
            &types::new_id("history")?,
            1_000 + seq as i64,
            seen,
            body,
        )?;
        let env = Envelope::plain(&event)?;
        let hash = event_log::chain_hash(SPACE, seq, &prev, &env.wire_hash())?;
        rows.push(Sequenced {
            seq,
            prev,
            hash,
            env,
        });
        Ok(())
    }
    async fn gate(&self, from: u64) {
        if from >= PAGE as u64 && !self.paused.swap(true, Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.acquire().await.unwrap().forget();
        }
    }
    pub(super) async fn raw(&self, from: u64, limit: usize) -> Vec<Sequenced> {
        self.gate(from).await;
        self.rows
            .lock()
            .unwrap()
            .0
            .iter()
            .filter(|ev| ev.seq >= from)
            .take(limit)
            .cloned()
            .collect()
    }
    pub(super) async fn visible(&self, who: &str, from: u64, limit: usize) -> Option<VisiblePage> {
        self.gate(from).await;
        if who != self.reader {
            return None;
        }
        let rows = self.rows.lock().unwrap();
        Some(VisiblePage {
            events: rows
                .0
                .iter()
                .filter(|ev| ev.seq >= from && rows.1.is_none_or(|last| ev.seq <= last))
                .take(limit)
                .cloned()
                .collect(),
            through: rows.1,
        })
    }
}

#[tokio::test]
async fn removal_between_sync_pages_hides_the_final_plaintext_tail(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let reader = Author::device(&Signer::generate()?, Signer::generate()?);
    let owner = Author::root(Signer::generate()?);
    let history = Arc::new(History {
        reader: reader.identity.clone(),
        rows: Default::default(),
        paused: AtomicBool::new(false),
        entered: Notify::new(),
        release: Semaphore::new(0),
    });
    {
        let mut rows = history.rows.lock().unwrap();
        History::push(
            &mut rows.0,
            &owner,
            EventBody::SpaceCreated {
                title: "Before removal".into(),
                kind: types::SpaceKind::Group,
                privacy: types::Privacy::Closed,
            },
        )?;
        History::push(
            &mut rows.0,
            &owner,
            EventBody::MemberAdded {
                identity: reader.identity.clone(),
                role: Role::Member,
            },
        )?;
        while rows.0.len() < PAGE {
            let index = rows.0.len();
            History::push(&mut rows.0, &owner, message(index))?;
        }
    }
    *test.log.sync_spaces.lock().unwrap() = Some(vec![SPACE.into()]);
    test.log
        .sync_members
        .lock()
        .unwrap()
        .insert((SPACE.into(), reader.identity.clone()));
    *test.log.sync_history.lock().unwrap() = Some(history.clone());
    let mut socket = connect(&test.ws_url, &reader).await;
    request(
        &mut socket,
        1,
        Op::Register {
            profile: profile(&reader, "qa_page_removal"),
        },
    )
    .await
    .unwrap();
    send(
        &mut socket,
        ClientFrame::Sync {
            cursors: Vec::new(),
            all: true,
        },
    )
    .await;
    let mut seen = Vec::new();
    for expected in 0..PAGE {
        let Some(ServerFrame::Event { ev }) = recv(&mut socket).await else {
            panic!("ordered first page");
        };
        assert_eq!(ev.seq, expected as u64);
        seen.push(ev);
    }
    tokio::time::timeout(Duration::from_secs(3), history.entered.notified())
        .await
        .unwrap();
    {
        let mut rows = history.rows.lock().unwrap();
        History::push(
            &mut rows.0,
            &owner,
            EventBody::MemberRemoved {
                identity: reader.identity.clone(),
            },
        )?;
        rows.1 = Some(PAGE as u64);
        History::push(&mut rows.0, &owner, message(999_999))?;
    }
    test.log
        .sync_members
        .lock()
        .unwrap()
        .remove(&(SPACE.into(), reader.identity.clone()));
    history.release.add_permits(1);
    loop {
        match recv(&mut socket).await.unwrap() {
            ServerFrame::Event { ev } => {
                assert!(
                    ev.seq <= PAGE as u64,
                    "post-removal plaintext escaped through stale initial membership"
                );
                seen.push(ev);
            }
            ServerFrame::SyncDone => break,
            other => panic!("unexpected sync response {other:?}"),
        }
    }
    assert_eq!(seen.len(), PAGE + 1);
    {
        let rows = history.rows.lock().unwrap();
        assert_eq!(rows.0.len(), PAGE + 2, "private tail really exists");
        for (actual, canonical) in seen.iter().zip(&rows.0) {
            assert_eq!(actual.encode(), canonical.encode());
        }
    }
    socket.close(None).await.unwrap();
    test.close().await;
    Ok(())
}

fn message(i: usize) -> EventBody {
    EventBody::MessagePosted {
        message: format!("m{i}"),
        text: format!("private history {i}"),
        attaches: None,
        reply: None,
        media: None,
        forwarded: false,
        forwarded_author: None,
    }
}
