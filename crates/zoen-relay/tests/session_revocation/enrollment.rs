use super::*;

#[tokio::test]
async fn enrollment_and_link_box_delivery_commit_or_rollback_together(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let first = Author::device(&root, Signer::generate()?);
    let next = Author::device(&root, Signer::generate()?);
    let mut controller = connect(&test.ws_url, &first).await;
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&first, "ana"),
        },
    )
    .await
    .unwrap();
    let box_id = Signer::generate()?.id();
    let pending = Author::device(&next.key, next.key.clone());
    let mut waiting = connect(&test.ws_url, &pending).await;
    assert_eq!(
        request(&mut waiting, 1, Op::FetchLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Link(None)
    );
    request(&mut controller, 2, link_op(&next, &box_id))
        .await
        .unwrap();
    // The link waiter authenticates as its own key, even after the sponsor enrolls
    // that key under the account. It must still be able to fetch the identity box.
    assert_eq!(
        request(&mut waiting, 2, Op::FetchLink { id: box_id.clone() })
            .await
            .unwrap(),
        Reply::Link(Some(vec![1]))
    );
    assert!(request(
        &mut waiting,
        3,
        Op::Register {
            profile: profile(&pending, "stolen")
        }
    )
    .await
    .is_err());
    assert!(
        !zoen_relay::db::is_registered(&test.state.pool, &pending.identity)
            .await
            .unwrap()
    );
    let _ = waiting.close(None).await;
    let mut linked = connect(&test.ws_url, &next).await;
    // Retry-safe enrollment keeps an already-active certificate, while duplicate box
    // delivery aborts the whole transaction and cannot leave a third device enrolled.
    let third = Author::device(&root, Signer::generate()?);
    let occupied_id = Signer::generate()?.id();
    request(
        &mut controller,
        3,
        Op::DeliverLink {
            id: occupied_id.clone(),
            sealed: vec![1],
            device: None,
        },
    )
    .await
    .unwrap();
    assert!(request(&mut controller, 4, link_op(&third, &occupied_id))
        .await
        .is_err());
    assert_device_absent(&test, &third).await;
    assert_eq!(
        zoen_relay::db::read_link_box(&test.state.pool, &occupied_id)
            .await
            .unwrap(),
        Some(vec![1])
    );
    let invalid_id = Signer::generate()?.id();
    let bad_cert = Author::device(&Signer::generate()?, Signer::generate()?);
    assert!(request(&mut controller, 5, link_op(&bad_cert, &invalid_id))
        .await
        .is_err());
    assert!(zoen_relay::db::read_link_box(&test.state.pool, &invalid_id)
        .await
        .unwrap()
        .is_none());
    assert_device_absent(&test, &bad_cert).await;
    // History manifests still work without a certificate and do not enroll any key.
    let history_id = Signer::generate()?.id();
    request(
        &mut controller,
        6,
        Op::DeliverLink {
            id: history_id.clone(),
            sealed: vec![2],
            device: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        zoen_relay::db::read_link_box(&test.state.pool, &history_id)
            .await
            .unwrap(),
        Some(vec![2])
    );
    let mut tx = test.state.pool.begin().await.unwrap();
    zoen_relay::db::enroll_device(
        &mut tx,
        &root.id(),
        next.device.as_ref().unwrap(),
        next.cert.as_ref().unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let _ = controller.close(None).await;
    let _ = linked.close(None).await;
    test.close().await;
    Ok(())
}

#[tokio::test]
async fn an_enrollment_retry_can_finish_while_a_backup_writer_waits_for_its_vault(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let device = Author::device(&root, Signer::generate()?);
    let mut controller = connect(&test.ws_url, &device).await;
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&device, "ana"),
        },
    )
    .await
    .unwrap();
    let _ = controller.close(None).await;
    // PR33 has no backup tables. This row reproduces the authoritative vault lock,
    // while the enrollment helper and both device row fences use the real directory.
    sqlx::query("CREATE TABLE enrollment_test_vault (identity TEXT PRIMARY KEY, revision INTEGER NOT NULL DEFAULT 0)")
        .execute(&test.state.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO enrollment_test_vault (identity) VALUES ($1)")
        .bind(root.id())
        .execute(&test.state.pool)
        .await
        .unwrap();
    let last_seen: String =
        sqlx::query_scalar("SELECT last_seen::text FROM devices WHERE device = $1")
            .bind(device.device.as_ref().unwrap())
            .fetch_one(&test.state.pool)
            .await
            .unwrap();
    // One slot in the application's main pool; the other PostgreSQL connection models
    // a concurrently authorized writer and exposes any nested pool acquisition.
    let admin_url = std::env::var("ZOEN_TEST_PG").unwrap();
    let url = format!(
        "{}/{}",
        admin_url.rsplit_once('/').unwrap().0,
        test.database
    );
    let mut writing = PgConnection::connect(&url).await.unwrap();
    let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut writing)
        .await
        .unwrap();
    let mut restore = test.state.pool.begin().await.unwrap();
    let restore_pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM enrollment_test_vault WHERE identity = $1 FOR UPDATE",
    )
    .bind(root.id())
    .fetch_one(&mut *restore)
    .await
    .unwrap();
    let (ready, acquired) = oneshot::channel();
    let identity = root.id();
    let device_key = device.device.clone().unwrap();
    let writer = tokio::spawn(async move {
        let mut tx = writing.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM devices WHERE identity = $1 AND device = $2 AND revoked_at IS NULL FOR SHARE")
            .bind(&identity)
            .bind(device_key)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        ready.send(()).unwrap();
        sqlx::query("UPDATE enrollment_test_vault SET revision = revision + 1 WHERE identity = $1")
            .bind(identity)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    });
    acquired.await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let blocked_by: Vec<i32> = sqlx::query_scalar("SELECT pg_blocking_pids($1)")
                .bind(writer_pid)
                .fetch_one(&mut test.admin)
                .await
                .unwrap();
            if blocked_by.contains(&restore_pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("writer did not wait for the held vault row");
    tokio::time::timeout(
        Duration::from_secs(2),
        zoen_relay::db::enroll_device(
            &mut restore,
            &root.id(),
            device.device.as_ref().unwrap(),
            device.cert.as_ref().unwrap(),
        ),
    )
    .await
    .expect("enrollment retry waited for the writer's compatible device fence")
    .unwrap();
    restore.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), writer)
        .await
        .unwrap()
        .unwrap();
    let revision: i32 =
        sqlx::query_scalar("SELECT revision FROM enrollment_test_vault WHERE identity = $1")
            .bind(root.id())
            .fetch_one(&test.state.pool)
            .await
            .unwrap();
    assert_eq!(revision, 1);
    let after: String = sqlx::query_scalar("SELECT last_seen::text FROM devices WHERE device = $1")
        .bind(device.device.as_ref().unwrap())
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(
        after, last_seen,
        "an enrollment retry mutated the device row"
    );
    test.close().await;
    Ok(())
}
