#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_claim_reply_lost_across_restart_spends_exactly_one_package() {
    let w = World::new("kp_claim_loss").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    assert_eq!(bruno.stock(&w).await, 32);
    bruno.core.stop_sync();
    let mut proxy = ClaimProxy::new(w.port, Boundary::Reply).await;
    let mut ana = Device::new(&w, "ana", &proxy.url);
    ana.start().await;
    let space = ana
        .core
        .create_group_with(
            "Lost reply".into(),
            vec![bruno.account.identity_id.clone()],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    let (operation, targets) = next_request(&mut proxy).await;
    assert_eq!(targets, vec![bruno.account.identity_id.clone()]);
    let (reply_operation, first, held) = next_reply(&mut proxy).await;
    assert_eq!(reply_operation, operation);
    assert!(held, "the first real committed reply must be withheld");
    assert_eq!(first.len(), 1);
    assert_eq!(bruno.stock(&w).await, 31);
    assert_eq!(
        w.count("SELECT count(*) FROM key_package_claim_receipts")
            .await,
        1
    );
    ana.core.stop_sync();
    proxy.disconnect.notify_one();
    ana.reopen().await;
    ana.core.start_sync(None).unwrap();
    let (retry, retry_targets) = next_request(&mut proxy).await;
    assert_eq!((retry, retry_targets), (operation.clone(), targets));
    let (_, replay, held) = next_reply(&mut proxy).await;
    assert!(!held);
    assert_eq!(
        replay, first,
        "receipt must return the same signed package bytes"
    );
    ana.core.wait_until_settled(8000).await.unwrap();
    assert_eq!(
        bruno.stock(&w).await,
        31,
        "retry must not consume a second package"
    );
    assert_eq!(
        w.count("SELECT count(*) FROM key_package_claim_receipts")
            .await,
        1
    );
    ana.close_database().await;
    let db = rusqlite::Connection::open(ana.home.join("zoen.sqlite")).unwrap();
    let pending: i64 = db
        .query_row(
            "SELECT count(*) FROM meta WHERE key = ?1",
            [format!("mls.claim:{space}")],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pending, 0, "staged MLS commit atomically retires its claim");
    drop(db);
    ana.reopen().await;
    ana.start().await;
    readable(&w, &ana, &bruno, &space).await;
    bruno.core.stop_sync();
    let mut claimant = RawClient::reconnect(&w.relay_url(), ana.author()).await;
    let mut db = PgConnection::connect(&w.db_url).await.unwrap();
    for i in 0..24 {
        let time: i64 = sqlx::query_scalar(
            "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
        )
        .fetch_one(&mut db)
        .await
        .unwrap();
        assert_eq!(
            claim(
                &mut claimant,
                vec![bruno.account.identity_id.clone()],
                &operation_at(time, 500 + i)
            )
            .await
            .unwrap()
            .len(),
            1
        );
    }
    assert_eq!(bruno.stock(&w).await, 7);
    let started = tokio::time::Instant::now();
    bruno.start().await;
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "refill exceeded unchanged twenty-second gate"
    );
    assert_eq!(
        bruno.stock(&w).await,
        32,
        "the real recipient replenishes the same stock target after replay"
    );
    readable(&w, &ana, &bruno, &space).await;
    assert_eq!(bruno.stock(&w).await, 32);
    eprintln!("durable causal treatment: same operation={operation}; same package bytes; committed31 -> retried31 -> low7 -> refilled32; MLS messages opened before and after refill");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn group_cli_deadline_cannot_report_an_unissued_claim_as_complete() {
    let w = World::new("kp_claim_unissued").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let mut proxy = ClaimProxy::new(w.port, Boundary::Request).await;
    let mut ana = Device::new(&w, "ana", &proxy.url);
    ana.start().await;
    ana.close_database().await;
    // An immutable legacy CLI artifact may run this same strict regression for
    // before/after evidence; the normal gate always uses this package's real CLI.
    let cli = std::env::var_os("ZOEN_TEST_GROUP_CLI_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_zoen").into());
    let mut child = Command::new(cli)
        .arg("--home")
        .arg(&ana.home)
        .args(["group", "Unissued", "@bruno"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (operation, targets) = next_request(&mut proxy).await;
    assert_eq!(targets, vec![bruno.account.identity_id.clone()]);
    assert_eq!(
        bruno.stock(&w).await,
        32,
        "the real request has not reached the relay"
    );
    // Default CLI deadline is unchanged; the held transport makes it deterministic.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(9);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("group ignored its default eight-second deadline");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        !output.status.success(),
        "unissued claim falsely completed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("did not finish before the deadline"));
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
    assert_eq!(bruno.stock(&w).await, 32);
    proxy.disconnect.notify_one();
    ana.reopen().await;
    ana.core.start_sync(None).unwrap();
    let (retry, retry_targets) = next_request(&mut proxy).await;
    assert_eq!((retry, retry_targets), (operation, targets));
    let (_, records, held) = next_reply(&mut proxy).await;
    assert!(!held);
    assert_eq!(records.len(), 1);
    ana.core.wait_until_settled(8000).await.unwrap();
    assert_eq!(bruno.stock(&w).await, 31);
    let space = ana
        .core
        .spaces()
        .into_iter()
        .find(|s| s.title == "Unissued")
        .unwrap()
        .id;
    readable(&w, &ana, &bruno, &space).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_claim_schedules_work_and_cannot_look_idle_before_retry() {
    let w = World::new("kp_claim_retry_idle").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    // Enroll the core's actual identity/device and signed agreement key, without
    // starting its package publisher. Removing a previously published last-resort
    // marker would contradict the core's persisted publication state.
    let author = bruno.author();
    let mut enrollment = RawClient::reconnect(&w.relay_url(), author.clone()).await;
    assert!(matches!(
        enrollment
            .request(Op::Register {
                profile: Identity {
                    id: bruno.account.identity_id.clone(),
                    kind: IdentityKind::Person,
                    name: bruno.account.name.clone(),
                    handle: bruno.account.handle.clone(),
                    tint_hex: "#000".into(),
                    glyph: None,
                    owner: None,
                    owner_proof: None,
                    bio: String::new(),
                },
            })
            .await
            .unwrap(),
        Reply::Registered(profile) if profile.id == bruno.account.identity_id
    ));
    let secret = bruno
        .vault
        .load("zoen.agreement.v1".into())
        .unwrap()
        .try_into()
        .unwrap();
    let public = AgreementKey::from_secret(secret).public_hex();
    assert!(matches!(
        enrollment
            .request(Op::PublishAgreementKey {
                signed: sign_agreement(&author, &public).unwrap(),
                public,
            })
            .await
            .unwrap(),
        Reply::Done
    ));
    drop(enrollment);
    assert_eq!(bruno.stock(&w).await, 0);
    let mut proxy = ClaimProxy::new(w.port, Boundary::ReplyThenRequest).await;
    let ana = Device::new(&w, "ana", &proxy.url);
    ana.start().await;
    let bootstrap_deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let space = ana
        .core
        .create_group("Scheduled retry".into(), vec![])
        .unwrap();
    // Observe the owner's first confirmed recovery commit. An idle outbox can
    // precede its staging; adding Bruno then would let him recover directly,
    // without needing the owner's scheduled second key-package claim.
    while !ana
        .core
        .group_keys(space.clone())
        .is_some_and(|keys| keys.epoch > 0)
    {
        assert!(
            tokio::time::Instant::now() < bootstrap_deadline,
            "owner recovery context was not confirmed within eight seconds"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ana.core
        .wait_until_settled(
            bootstrap_deadline
                .saturating_duration_since(tokio::time::Instant::now())
                .as_millis() as u64,
        )
        .await
        .unwrap();
    ana.core
        .add_member(space.clone(), bruno.account.identity_id.clone())
        .unwrap();
    let (empty_operation, _) = next_request(&mut proxy).await;
    let (_, records, held) = next_reply(&mut proxy).await;
    assert!(held);
    assert!(records.is_empty());
    // The held claim is independent of the initial group's event acknowledgments.
    // Observe that outbox draining has finished before testing an idle-looking retry.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let connection = ana.core.connection();
        if connection.synced && connection.pending == 0 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "initial group outbox did not drain: {connection:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    proxy.release.notify_one();
    assert!(ana.core.wait_until_settled(200).await.is_err());
    let connection = ana.core.connection();
    assert!(connection.synced, "{connection:?}");
    assert_eq!(
        connection.pending, 0,
        "a scheduled claim retry exists outside the event outbox"
    );
    // Starting Bruno also catches up the joined group. Its settled return can
    // occur after Ana's retry has legitimately consumed one package. Hold that
    // request before relay admission to observe publication and consumption apart.
    let publication_deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    bruno.core.start_sync(None).unwrap();
    let (retry, _) = next_request(&mut proxy).await;
    assert_ne!(
        retry, empty_operation,
        "a completed empty claim must not replay emptiness forever"
    );
    while bruno.stock(&w).await != 32 {
        assert!(
            tokio::time::Instant::now() < publication_deadline,
            "Bruno did not publish 32 packages before the held retry"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(bruno.stock(&w).await, 32);
    proxy.release.notify_one();
    let (_, records, held) = next_reply(&mut proxy).await;
    assert!(!held);
    assert_eq!(records.len(), 1);
    ana.core.wait_until_settled(8000).await.unwrap();
    bruno.core.wait_until_settled(8000).await.unwrap();
    assert_eq!(bruno.stock(&w).await, 31);
    readable(&w, &ana, &bruno, &space).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn protocol_four_without_the_connected_relay_capability_refuses_claims() {
    let w = World::new("kp_claim_capability").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let mut proxy = ClaimProxy::new(w.port, Boundary::AbsentCapability).await;
    let ana = Device::new(&w, "ana", &proxy.url);
    let (refusals, mut errors) = tokio::sync::mpsc::unbounded_channel();
    ana.core
        .start_sync(Some(Arc::new(Refusals(refusals))))
        .unwrap();
    ana.core.wait_until_settled(8000).await.unwrap();
    ana.core
        .create_group_with(
            "Needs receipts".into(),
            vec![bruno.account.identity_id.clone()],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    let refusal = tokio::time::timeout(Duration::from_secs(8), errors.recv())
        .await
        .expect("the client did not report the negotiated capability refusal")
        .expect("the client listener stopped");
    assert!(
        refusal.contains("does not support durable key-package claims"),
        "{refusal}"
    );
    let error = ana.core.wait_until_settled(1000).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not support durable key-package claims"),
        "{error}"
    );
    assert!(
        proxy.events.try_recv().is_err(),
        "unsupported relay received a destructive claim"
    );
    assert_eq!(bruno.stock(&w).await, 32);
    assert_eq!(
        w.count("SELECT count(*) FROM key_package_claim_receipts")
            .await,
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn receipt_claims_progress_with_a_sixty_second_device_event_clock_skew(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "ZOEN_CLAIM_CLOCK_SKEW_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // new_ulid's monotonic event clock is process-wide. Isolate the skew so
        // other genuine-crypto journeys retain their normal device clocks.
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "receipt_claims_progress_with_a_sixty_second_device_event_clock_skew",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let w = World::new("kp_claim_clock_skew").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let mut proxy = ClaimProxy::new(w.port, Boundary::Reply).await;
    let ana = Device::new(&w, "ana", &proxy.url);
    ana.start().await;
    let skewed = types::new_ulid(now_ms() + 60_000)?;
    let space = ana
        .core
        .create_group_with(
            "Phone clock ahead".into(),
            vec![bruno.account.identity_id.clone()],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    let (operation, _) = next_request(&mut proxy).await;
    assert!(
        protocol::key_package_claim_time_ms(&operation).unwrap()
            < types::id_time_ms(&skewed).unwrap() - 50_000
    );
    let (_, records, held) = next_reply(&mut proxy).await;
    assert!(held);
    assert_eq!(records.len(), 1, "relay refused a normally skewed device");
    proxy.release.notify_one();
    ana.core.wait_until_settled(8000).await.unwrap();
    assert_eq!(bruno.stock(&w).await, 31);
    readable(&w, &ana, &bruno, &space).await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn legacy_claim_reply_loss_causally_spends_an_extra_package(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("kp_claim_legacy_loss").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let mut proxy = ClaimProxy::new(w.port, Boundary::Reply).await;
    let mut first_client = RawClient::connect(&proxy.url, "ana").await?;
    let author = first_client.author.clone();
    assert!(
        first_client
            .send_if_open(&ClientFrame::Req {
                id: 1,
                op: Op::ClaimKeyPackages {
                    ids: vec![bruno.account.identity_id.clone()],
                    operation_id: None,
                },
            })
            .await
    );
    assert!(matches!(
        proxy.next().await,
        Observed::Request {
            operation: None,
            ..
        }
    ));
    let Observed::Reply {
        operation: None,
        records: first,
        held: true,
    } = proxy.next().await
    else {
        panic!("real legacy reply must be committed and withheld")
    };
    assert_eq!(first.len(), 1);
    assert_eq!(bruno.stock(&w).await, 31);
    drop(first_client);
    proxy.disconnect.notify_one();
    let mut retry = RawClient::reconnect(&proxy.url, author).await;
    let Reply::KeyPackages(second) = retry
        .request(Op::ClaimKeyPackages {
            ids: vec![bruno.account.identity_id.clone()],
            operation_id: None,
        })
        .await
        .unwrap()
    else {
        panic!("legacy claim reply")
    };
    assert!(matches!(
        proxy.next().await,
        Observed::Request {
            operation: None,
            ..
        }
    ));
    assert!(matches!(
        proxy.next().await,
        Observed::Reply {
            operation: None,
            held: false,
            ..
        }
    ));
    assert_eq!(second.len(), 1);
    assert_ne!(first, second);
    assert_eq!(
        bruno.stock(&w).await,
        30,
        "unchanged legacy retry consumes a second real package"
    );
    assert_eq!(
        w.count("SELECT count(*) FROM key_package_claim_receipts")
            .await,
        0
    );
    eprintln!("legacy causal reproduction: committed31 -> retried30; different signed package bytes; this does not identify a historical CI reply loss");
    Ok(())
}
