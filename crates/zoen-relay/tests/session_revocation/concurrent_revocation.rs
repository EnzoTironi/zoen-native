use super::*;

async fn stalled_operation_revocation(
    publish: bool,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let (state, log, ws_url) = (&test.state, &test.log, &test.ws_url);
    let root = Signer::generate()?;
    let first = Author::device(&root, Signer::generate()?);
    let lost = Author::device(&root, Signer::generate()?);
    let mut controller = connect(ws_url, &first).await;
    send(
        &mut controller,
        ClientFrame::Req {
            id: 1,
            op: Op::Register {
                profile: Identity {
                    owner_proof: None,
                    id: root.id(),
                    kind: IdentityKind::Person,
                    name: "Ana".into(),
                    handle: "ana".into(),
                    tint_hex: "#123456".into(),
                    glyph: None,
                    owner: None,
                    bio: String::new(),
                },
            },
        },
    )
    .await;
    loop {
        if let ServerFrame::Res { id: 1, result } =
            recv(&mut controller).await.expect("controller closed")
        {
            result.unwrap();
            break;
        }
    }
    request(
        &mut controller,
        3,
        link_op(&lost, &Signer::generate()?.id()),
    )
    .await
    .unwrap();
    let mut target = connect(ws_url, &lost).await;
    let operation = if publish {
        ClientFrame::Publish {
            env: Envelope::plain(&lost.sign_event(
                "sp_before_revoke",
                "admitted-before-revocation",
                1,
                None,
                types::EventBody::SpaceCreated {
                    title: "Already admitted".into(),
                    kind: types::SpaceKind::Group,
                    privacy: types::Privacy::Closed,
                },
            )?),
        }
    } else {
        ClientFrame::Sync {
            cursors: Vec::new(),
            all: true,
        }
    };
    send(&mut target, operation).await;
    tokio::time::timeout(Duration::from_secs(3), log.entered.notified())
        .await
        .unwrap();
    assert!(log.reading.load(Ordering::Acquire));
    send(
        &mut controller,
        ClientFrame::Req {
            id: 2,
            op: Op::Unlink {
                device: lost.device.clone().unwrap(),
            },
        },
    )
    .await;
    loop {
        if let ServerFrame::Res { id: 2, result } =
            recv(&mut controller).await.expect("controller closed")
        {
            assert_eq!(result.unwrap(), Reply::Done);
            break;
        }
    }
    assert!(
        recv(&mut target).await.is_none(),
        "revoked stalled reader did not close"
    );
    tokio::time::timeout(Duration::from_secs(3), log.cancelled.notified())
        .await
        .unwrap();
    assert!(
        zoen_relay::db::device_revoked(&state.pool, lost.device.as_ref().unwrap())
            .await
            .unwrap()
    );
    assert!(
        !log.reading.load(Ordering::Acquire),
        "blocked read future survived closure"
    );
    if publish {
        assert!(log.saved.lock().unwrap().is_empty());
        log.release.add_permits(1);
        tokio::time::timeout(Duration::from_secs(3), log.finished.notified())
            .await
            .unwrap();
        assert_eq!(
            log.saved.lock().unwrap()[0].env.client_id(),
            "admitted-before-revocation"
        );
        // A key-package write admitted earlier could also finish after the deletion.
        // Future claims must never give a revoked device's retained packages out.
        assert!(zoen_relay::db::put_key_packages(
            &state.pool,
            &root.id(),
            lost.device.as_ref().unwrap(),
            &[],
            Some(b"late-key-package")
        )
        .await
        .unwrap());
        assert!(
            zoen_relay::db::claim_key_packages(&state.pool, &[root.id()])
                .await
                .unwrap()
                .is_empty()
        );
    }
    let _ = controller.close(None).await;
    test.close().await;
    Ok(())
}

/// A stalled bounded presence lookup must not consume the login/frame budget.
#[tokio::test]
async fn a_stalled_sync_cannot_block_online_device_revocation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    stalled_operation_revocation(false).await?;
    Ok(())
}

#[tokio::test]
async fn an_admitted_queued_publish_can_finish_without_blocking_revocation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    stalled_operation_revocation(true).await?;
    Ok(())
}
