//! Original sealed data survives response loss; a fetch is not a new publication.
use super::*;

#[tokio::test]
async fn lost_fetch_response_can_retry_the_same_enrolled_pairing_on_another_connection(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let sponsor = Author::device(&root, Signer::generate()?);
    let next = Author::device(&root, Signer::generate()?);
    let pending = Author::device(&next.key, next.key.clone());
    let mut controller = connect(&test.ws_url, &sponsor).await;
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&sponsor, "ana"),
        },
    )
    .await
    .unwrap();
    let id = Signer::generate()?.id();
    request(&mut controller, 2, link_op(&next, &id))
        .await
        .unwrap();
    let created: String =
        sqlx::query_scalar("SELECT created_at::text FROM link_boxes WHERE id = $1")
            .bind(&id)
            .fetch_one(&test.state.pool)
            .await
            .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("ws://{}", listener.local_addr().unwrap());
    let (dropped, received) = oneshot::channel();
    let proxy = tokio::spawn(response_loss::drop_matching(
        listener,
        test.ws_url.clone(),
        |frame| {
            matches!(
                frame,
                ServerFrame::Res {
                    result: Ok(Reply::Link(Some(_))),
                    ..
                }
            )
        },
        dropped,
    ));
    let mut waiting = connect(&proxy_url, &pending).await;
    send(
        &mut waiting,
        ClientFrame::Req {
            id: 1,
            op: Op::FetchLink { id: id.clone() },
        },
    )
    .await;
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), received).await.unwrap().unwrap(),
        ServerFrame::Res { result: Ok(Reply::Link(Some(sealed))), .. } if sealed == vec![1]
    ));
    assert!(
        recv(&mut waiting).await.is_none(),
        "lost response reached recipient"
    );
    tokio::time::timeout(Duration::from_secs(3), proxy)
        .await
        .unwrap()
        .unwrap();
    let mut retry = connect(&test.ws_url, &pending).await;
    for request_id in 1..=2 {
        assert_eq!(
            request(&mut retry, request_id, Op::FetchLink { id: id.clone() })
                .await
                .unwrap(),
            Reply::Link(Some(vec![1]))
        );
    }
    let after: String = sqlx::query_scalar("SELECT created_at::text FROM link_boxes WHERE id = $1")
        .bind(&id)
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(
        created, after,
        "fetch or reconnect extended publication TTL"
    );
    let third = Author::device(&root, Signer::generate()?);
    assert!(request(&mut controller, 3, link_op(&third, &id))
        .await
        .is_err());
    assert_device_absent(&test, &third).await;
    assert_eq!(
        request(&mut retry, 3, Op::FetchLink { id }).await.unwrap(),
        Reply::Link(Some(vec![1]))
    );
    let _ = retry.close(None).await;
    let _ = controller.close(None).await;
    test.close().await;
    Ok(())
}

#[tokio::test]
async fn repeated_history_fetch_never_renews_expiry_or_creates_an_enrollment(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let sponsor = Author::device(&root, Signer::generate()?);
    let mut controller = connect(&test.ws_url, &sponsor).await;
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&sponsor, "ana"),
        },
    )
    .await
    .unwrap();
    let id = Signer::generate()?.id();
    request(
        &mut controller,
        2,
        Op::DeliverLink {
            id: id.clone(),
            sealed: vec![2, 3],
            device: None,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE link_boxes SET created_at = now() - interval '14 minutes' WHERE id = $1")
        .bind(&id)
        .execute(&test.state.pool)
        .await
        .unwrap();
    let created: String =
        sqlx::query_scalar("SELECT created_at::text FROM link_boxes WHERE id = $1")
            .bind(&id)
            .fetch_one(&test.state.pool)
            .await
            .unwrap();
    for request_id in 3..=4 {
        assert_eq!(
            request(
                &mut controller,
                request_id,
                Op::FetchLink { id: id.clone() }
            )
            .await
            .unwrap(),
            Reply::Link(Some(vec![2, 3]))
        );
    }
    let after: String = sqlx::query_scalar("SELECT created_at::text FROM link_boxes WHERE id = $1")
        .bind(&id)
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(created, after);
    let devices: i64 = sqlx::query_scalar("SELECT count(*) FROM devices WHERE identity = $1")
        .bind(root.id())
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(devices, 1, "history publication or fetching enrolled a key");
    sqlx::query("UPDATE link_boxes SET created_at = now() - interval '16 minutes' WHERE id = $1")
        .bind(&id)
        .execute(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(
        request(&mut controller, 5, Op::FetchLink { id: id.clone() })
            .await
            .unwrap(),
        Reply::Link(None)
    );
    let unknown = Signer::generate()?.id();
    assert_eq!(
        request(&mut controller, 6, Op::FetchLink { id: unknown })
            .await
            .unwrap(),
        Reply::Link(None)
    );
    request(
        &mut controller,
        7,
        Op::DeliverLink {
            id: id.clone(),
            sealed: vec![4],
            device: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        request(&mut controller, 8, Op::FetchLink { id })
            .await
            .unwrap(),
        Reply::Link(Some(vec![4]))
    );
    let _ = controller.close(None).await;
    test.close().await;
    Ok(())
}
