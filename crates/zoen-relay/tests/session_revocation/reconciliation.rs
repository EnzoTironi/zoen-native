//! Real commit/lost ACK/replay: equality is scoped to the original publisher/target.
use super::*;

async fn registered(
    test: &TestRelay,
    signer: &Signer,
    handle: &str,
) -> std::result::Result<(Author, Socket), Box<dyn std::error::Error>> {
    let author = Author::device(signer, Signer::generate()?);
    let mut socket = connect(&test.ws_url, &author).await;
    request(
        &mut socket,
        1,
        Op::Register {
            profile: profile(&author, handle),
        },
    )
    .await
    .unwrap();
    Ok((author, socket))
}
fn deliver(target: &Author, id: &str, bytes: Vec<u8>) -> Op {
    Op::DeliverLink {
        id: id.into(),
        sealed: bytes,
        device: Some(DeviceCertificate {
            device: target.device.clone().unwrap(),
            cert: target.cert.clone().unwrap(),
        }),
    }
}
async fn created(test: &TestRelay, id: &str) -> String {
    sqlx::query_scalar("SELECT created_at::text FROM link_boxes WHERE id=$1")
        .bind(id)
        .fetch_one(&test.state.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn committed_enrollment_lost_ack_can_replay_exact_original_without_refreshing_expiry(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let (sponsor, mut controller) = registered(&test, &root, "ana").await?;
    let target = Author::device(&root, Signer::generate()?);
    let id = Signer::generate()?.id();
    let bytes = vec![1, 4, 9];
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (dropped, received) = oneshot::channel();
    let proxy = tokio::spawn(response_loss::drop_matching(
        listener,
        test.ws_url.clone(),
        |frame| {
            matches!(
                frame,
                ServerFrame::Res {
                    result: Ok(Reply::Done),
                    ..
                }
            )
        },
        dropped,
    ));
    let mut original = connect(&url, &sponsor).await;
    send(
        &mut original,
        ClientFrame::Req {
            id: 1,
            op: deliver(&target, &id, bytes.clone()),
        },
    )
    .await;
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), received)
            .await
            .unwrap()
            .unwrap(),
        ServerFrame::Res {
            result: Ok(Reply::Done),
            ..
        }
    ));
    assert!(
        recv(&mut original).await.is_none(),
        "lost ACK reached sender"
    );
    tokio::time::timeout(Duration::from_secs(3), proxy)
        .await
        .unwrap()
        .unwrap();
    let timestamp = created(&test, &id).await;
    let mut retry = connect(&test.ws_url, &sponsor).await;
    assert_eq!(
        request(&mut retry, 1, deliver(&target, &id, bytes.clone()))
            .await
            .unwrap(),
        Reply::Done
    );
    assert_eq!(created(&test, &id).await, timestamp);
    assert_eq!(
        zoen_relay::db::read_link_box(&test.state.pool, &id)
            .await
            .unwrap(),
        Some(bytes.clone())
    );
    let enrolled: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM devices WHERE identity=$1 AND device=$2 AND revoked_at IS NULL",
    )
    .bind(root.id())
    .bind(target.device.as_deref())
    .fetch_one(&test.state.pool)
    .await
    .unwrap();
    assert_eq!(enrolled, 1);
    assert!(request(&mut retry, 2, deliver(&target, &id, vec![2, 4, 9]))
        .await
        .is_err());
    let third = Author::device(&root, Signer::generate()?);
    assert!(request(&mut retry, 3, deliver(&third, &id, bytes.clone()))
        .await
        .is_err());
    assert_device_absent(&test, &third).await;
    let wrong = Author::device(&Signer::generate()?, Signer::generate()?);
    assert!(request(&mut retry, 4, deliver(&wrong, &id, bytes.clone()))
        .await
        .is_err());
    assert_device_absent(&test, &wrong).await;
    // Even another active sponsor of the same owner cannot claim this original.
    let other_id = Signer::generate()?.id();
    request(&mut controller, 2, deliver(&third, &other_id, vec![5]))
        .await
        .unwrap();
    let mut sibling = connect(&test.ws_url, &third).await;
    assert!(
        request(&mut sibling, 1, deliver(&target, &id, bytes.clone()))
            .await
            .is_err()
    );
    let (foreign, mut foreign_socket) = registered(&test, &Signer::generate()?, "bruno").await?;
    assert!(
        request(&mut foreign_socket, 2, deliver(&target, &id, bytes.clone()))
            .await
            .is_err()
    );
    assert_ne!(foreign.identity, root.id());
    assert_eq!(created(&test, &id).await, timestamp);
    request(
        &mut controller,
        3,
        Op::Unlink {
            device: target.device.clone().unwrap(),
        },
    )
    .await
    .unwrap();
    assert!(request(&mut retry, 5, deliver(&target, &id, bytes.clone()))
        .await
        .is_err());
    let revoked: bool =
        sqlx::query_scalar("SELECT revoked_at IS NOT NULL FROM devices WHERE device=$1")
            .bind(target.device.as_deref())
            .fetch_one(&test.state.pool)
            .await
            .unwrap();
    assert!(revoked, "exact replay cannot revive a revoked target");
    assert_eq!(created(&test, &id).await, timestamp);
    assert_eq!(
        zoen_relay::db::read_link_box(&test.state.pool, &id)
            .await
            .unwrap(),
        Some(bytes)
    );
    let _ = foreign_socket.close(None).await;
    let _ = sibling.close(None).await;
    let _ = retry.close(None).await;
    let _ = controller.close(None).await;
    test.close().await;
    Ok(())
}

#[tokio::test]
async fn history_or_legacy_payload_equality_cannot_be_upgraded_to_enrollment(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let (_, mut controller) = registered(&test, &root, "ana").await?;
    let target = Author::device(&root, Signer::generate()?);
    let history = Signer::generate()?.id();
    request(
        &mut controller,
        2,
        Op::DeliverLink {
            id: history.clone(),
            sealed: vec![1],
            device: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        request(
            &mut controller,
            3,
            Op::DeliverLink {
                id: history.clone(),
                sealed: vec![1],
                device: None
            }
        )
        .await
        .unwrap(),
        Reply::Done
    );
    assert!(
        request(&mut controller, 4, deliver(&target, &history, vec![1]))
            .await
            .is_err()
    );
    assert_device_absent(&test, &target).await;
    let legacy = Signer::generate()?.id();
    sqlx::query("INSERT INTO link_boxes (id,sealed) VALUES ($1,$2)")
        .bind(&legacy)
        .bind(vec![1u8])
        .execute(&test.state.pool)
        .await
        .unwrap();
    assert!(
        request(&mut controller, 5, deliver(&target, &legacy, vec![1]))
            .await
            .is_err()
    );
    assert_device_absent(&test, &target).await;
    assert!(request(
        &mut controller,
        6,
        Op::DeliverLink {
            id: legacy.clone(),
            sealed: vec![1],
            device: None
        }
    )
    .await
    .is_err());
    assert_eq!(
        zoen_relay::db::read_link_box(&test.state.pool, &legacy)
            .await
            .unwrap(),
        Some(vec![1])
    );
    let _ = controller.close(None).await;
    test.close().await;
    Ok(())
}

#[tokio::test]
async fn expired_republication_is_new_work_under_current_admission_and_revocation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let (sponsor, mut controller) = registered(&test, &root, "ana").await?;
    let target = Author::device(&root, Signer::generate()?);
    let id = Signer::generate()?.id();
    request(&mut controller, 2, deliver(&target, &id, vec![1]))
        .await
        .unwrap();
    sqlx::query("UPDATE link_boxes SET created_at=now()-interval '16 minutes' WHERE id=$1")
        .bind(&id)
        .execute(&test.state.pool)
        .await
        .unwrap();
    let old = created(&test, &id).await;
    request(&mut controller, 3, deliver(&target, &id, vec![2]))
        .await
        .unwrap();
    assert_ne!(
        created(&test, &id).await,
        old,
        "expired reuse is a new publication"
    );
    assert_eq!(
        zoen_relay::db::read_link_box(&test.state.pool, &id)
            .await
            .unwrap(),
        Some(vec![2])
    );
    request(
        &mut controller,
        4,
        Op::Unlink {
            device: target.device.clone().unwrap(),
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE link_boxes SET created_at=now()-interval '16 minutes' WHERE id=$1")
        .bind(&id)
        .execute(&test.state.pool)
        .await
        .unwrap();
    assert!(request(&mut controller, 5, deliver(&target, &id, vec![2]))
        .await
        .is_err());
    assert!(zoen_relay::db::read_link_box(&test.state.pool, &id)
        .await
        .unwrap()
        .is_none());
    // A revoked sponsor also loses ordinary admission, before replay inspection.
    let live = Signer::generate()?.id();
    request(
        &mut controller,
        6,
        Op::DeliverLink {
            id: live.clone(),
            sealed: vec![3],
            device: None,
        },
    )
    .await
    .unwrap();
    // Commit the revocation explicitly; dropping a transaction cannot qualify it.
    let mut tx = test.state.pool.begin().await.unwrap();
    assert!(
        zoen_relay::db::revoke_device(&mut tx, &root.id(), sponsor.device.as_deref().unwrap())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    let (mut refused, result) = authenticate(&test.ws_url, &sponsor).await;
    assert!(!matches!(result, ServerFrame::Ready { .. }));
    assert_eq!(
        zoen_relay::db::read_link_box(&test.state.pool, &live)
            .await
            .unwrap(),
        Some(vec![3])
    );
    let _ = refused.close(None).await;
    let _ = controller.close(None).await;
    test.close().await;
    Ok(())
}
