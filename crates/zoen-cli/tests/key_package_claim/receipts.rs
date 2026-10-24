fn operation_at(at_ms: i64, nonce: u128) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut value = ((at_ms as u128) << 80) | nonce;
    let mut output = [b'0'; 26];
    for ch in output.iter_mut().rev() {
        *ch = ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(output.to_vec()).unwrap()
}

async fn claim(
    client: &mut RawClient,
    targets: Vec<String>,
    operation: &str,
) -> Result<Vec<KeyPackageRecord>, String> {
    match client
        .request(Op::ClaimKeyPackages {
            ids: targets,
            operation_id: Some(operation.into()),
        })
        .await?
    {
        Reply::KeyPackages(records) => Ok(records),
        reply => panic!("unexpected {reply:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn receipts_bind_canonical_targets_and_authenticated_actor(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("kp_claim_scope").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let source = Device::new(&w, "ana", &w.relay_url());
    source.start().await;
    source.core.stop_sync();
    let mut ana = RawClient::reconnect(&w.relay_url(), source.author()).await;
    let mut carol = RawClient::connect(&w.relay_url(), "carol").await?;
    let operation = operation_at(now_ms(), 1);
    for invalid in [
        bruno.account.identity_id.to_uppercase(),
        "x".repeat(900_000),
    ] {
        let invalid_target = claim(&mut ana, vec![invalid], &operation)
            .await
            .unwrap_err();
        assert!(invalid_target.contains("canonical identity"));
    }
    assert_eq!(
        w.count("SELECT count(*) FROM key_package_claim_receipts")
            .await,
        0
    );
    assert_eq!(bruno.stock(&w).await, 32);
    let first = claim(
        &mut ana,
        vec![bruno.account.identity_id.clone(), carol.identity()],
        &operation,
    )
    .await
    .unwrap();
    assert_eq!(first.len(), 1);
    let replay = claim(
        &mut ana,
        // More than fifty raw entries still represent the same two identities.
        // The durable limit applies after canonicalization, before any consumption.
        (0..62)
            .map(|i| {
                if i % 2 == 0 {
                    carol.identity()
                } else {
                    bruno.account.identity_id.clone()
                }
            })
            .collect(),
        &operation,
    )
    .await
    .unwrap();
    assert_eq!(
        first, replay,
        "order and duplicate aliases canonicalize to the same targets"
    );
    let error = claim(
        &mut ana,
        vec![bruno.account.identity_id.clone()],
        &operation,
    )
    .await
    .unwrap_err();
    assert!(error.contains("different targets"), "{error}");
    assert_eq!(bruno.stock(&w).await, 31);
    let other = claim(
        &mut carol,
        vec![bruno.account.identity_id.clone()],
        &operation,
    )
    .await
    .unwrap();
    assert_eq!(other.len(), 1);
    assert_ne!(
        other, first,
        "another authenticated identity cannot read Ana's receipt"
    );
    assert_eq!(bruno.stock(&w).await, 30);
    let mut again = RawClient::reconnect(&w.relay_url(), ana.author.clone()).await;
    assert_eq!(
        claim(
            &mut again,
            vec![carol.identity(), bruno.account.identity_id.clone()],
            &operation
        )
        .await
        .unwrap(),
        first
    );
    assert_eq!(bruno.stock(&w).await, 30);
    let root: [u8; 32] = source
        .vault
        .load("zoen.identity.v1".into())
        .unwrap()
        .try_into()
        .unwrap();
    let second = Author::device(&Signer::from_secret(&root), Signer::generate()?);
    ana.request(Op::DeliverLink {
        id: Signer::generate()?.id(),
        sealed: vec![1],
        device: Some(protocol::DeviceCertificate {
            device: second.device.clone().unwrap(),
            cert: second.cert.clone().unwrap(),
        }),
    })
    .await
    .unwrap();
    let mut other_device = RawClient::reconnect(&w.relay_url(), second).await;
    let independent = claim(
        &mut other_device,
        vec![bruno.account.identity_id.clone()],
        &operation,
    )
    .await
    .unwrap();
    assert_ne!(
        independent, first,
        "a different enrolled device has a separate operation scope"
    );
    assert_eq!(bruno.stock(&w).await, 29);
    // Revocation still fences outgoing receipt delivery and future admission.
    let device = ana.author.device.clone().unwrap();
    other_device.request(Op::Unlink { device }).await.unwrap();
    while ana.recv_or_close().await.is_some() {}
    let _ = again
        .send_if_open(&protocol::ClientFrame::Req {
            id: 99,
            op: Op::ClaimKeyPackages {
                ids: vec![bruno.account.identity_id.clone()],
                operation_id: Some(operation),
            },
        })
        .await;
    while let Some(frame) = again.recv_or_close().await {
        assert!(!matches!(
            frame,
            ServerFrame::Res {
                id: 99,
                result: Ok(Reply::KeyPackages(_))
            }
        ));
    }
    assert_eq!(bruno.stock(&w).await, 29);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_future_and_noncanonical_operations_never_consume_packages(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("kp_claim_expiry").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let mut ana = RawClient::connect(&w.relay_url(), "ana").await?;
    let now = now_ms();
    for operation in [
        operation_at(now - KEY_PACKAGE_CLAIM_TTL_MS - 1000, 1),
        operation_at(now + 60_000, 2),
        "Z0000000000000000000000000".into(),
        operation_at(now, 3).to_lowercase(),
        "prefix-00000000000000000000000000".into(),
    ] {
        assert!(claim(
            &mut ana,
            vec![bruno.account.identity_id.clone()],
            &operation
        )
        .await
        .is_err());
    }
    assert_eq!(bruno.stock(&w).await, 32);
    let expired = operation_at(now - KEY_PACKAGE_CLAIM_TTL_MS - 1000, 7);
    let mut db = PgConnection::connect(&w.db_url).await.unwrap();
    sqlx::query("INSERT INTO key_package_claim_receipts(source_identity,source_device,operation_id,targets,response,expires_at_ms) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(ana.identity()).bind(ana.author.device.clone().unwrap()).bind(&expired)
        .bind(vec![bruno.account.identity_id.clone()]).bind(ServerFrame::Res { id: 0, result: Ok(Reply::KeyPackages(Vec::new())) }.encode()).bind(now - 1000)
        .execute(&mut db).await.unwrap();
    // Same deletion performed by actor cleanup / indexed background GC. Absence of an
    // old receipt must not reinterpret an expired operation as a fresh destructive claim.
    sqlx::query("DELETE FROM key_package_claim_receipts WHERE expires_at_ms < $1")
        .bind(now)
        .execute(&mut db)
        .await
        .unwrap();
    assert_eq!(
        claim(&mut ana, vec![bruno.account.identity_id.clone()], &expired)
            .await
            .unwrap_err(),
        KEY_PACKAGE_CLAIM_EXPIRED
    );
    assert_eq!(bruno.stock(&w).await, 32);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn advertised_receipts_without_a_valid_challenge_clock_never_enable_claims() {
    for boundary in [Boundary::AbsentClock, Boundary::MalformedClock] {
        let w = World::new("kp_claim_invalid_clock").await;
        let bruno = Device::new(&w, "bruno", &w.relay_url());
        bruno.start().await;
        bruno.core.stop_sync();
        let mut proxy = ClaimProxy::new(w.port, boundary).await;
        let ana = Device::new(&w, "ana", &proxy.url);
        let (refusals, mut errors) = tokio::sync::mpsc::unbounded_channel();
        ana.core
            .start_sync(Some(Arc::new(Refusals(refusals))))
            .unwrap();
        ana.core.wait_until_settled(8000).await.unwrap();
        assert!(matches!(proxy.next().await, Observed::ClockRefreshed));
        ana.core
            .create_group_with(
                "Missing receipt clock".into(),
                vec![bruno.account.identity_id.clone()],
                PrivacyDto::EndToEnd,
            )
            .unwrap();
        let refusal = tokio::time::timeout(Duration::from_secs(8), errors.recv())
            .await
            .expect("the client did not report the negotiated clock refusal")
            .expect("the client listener stopped");
        assert!(refusal.contains("server clock"), "{refusal}");
        let error = ana.core.wait_until_settled(1000).await.unwrap_err();
        assert!(error.to_string().contains("server clock"), "{error}");
        // The initial Ping receives a genuine, valid-clock Pong. It must not enable
        // a clock/capability absent or malformed in this socket's Challenge.
        assert!(
            proxy.events.try_recv().is_err(),
            "invalid clock enabled a destructive claim"
        );
        assert_eq!(bruno.stock(&w).await, 32);
        assert_eq!(
            w.count("SELECT count(*) FROM key_package_claim_receipts")
                .await,
            0
        );
    }
}
