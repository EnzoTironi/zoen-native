use super::*;

pub(super) async fn journey() {
    let admin_url = std::env::var("ZOEN_TEST_PG").expect("Postgres");
    let db_name = format!("ownership_{}", types::new_id("t").to_lowercase());
    let mut admin = PgConnection::connect(&admin_url).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {db_name}")))
        .execute(&mut admin)
        .await
        .unwrap();
    let url = format!("{}/{db_name}", admin_url.rsplit_once('/').unwrap().0);
    let dir: PathBuf = std::env::temp_dir().join(&db_name);
    std::fs::create_dir_all(&dir).unwrap();
    let cluster = std::env::var("FDB_CLUSTER_FILE").unwrap();
    let db = Database::new(Some(&cluster)).unwrap();
    let root = Subspace::all().subspace(&("zoen", &db_name));
    let nats = std::env::var("ZOEN_NATS_URL").expect("NATS");
    let upstream = nats.strip_prefix("nats://").unwrap().to_string();
    let proxy = NatsProxy::start(0, upstream.clone()).await;
    let proxy_port = proxy.port;
    let mut a = Node::start(&url, &db_name, &dir, "a", &proxy.url()).await;
    let probe = types::new_id("sp");
    let alpha = current(&db, &root, &probe).await.fence.owner;
    let author = Author::device(&Signer::generate(), Signer::generate());
    let mut device_a = Device::connect(a.port, author.clone(), true).await;
    let mut b = Node::start(&url, &db_name, &dir, "b", &nats).await;
    let beta = db
        .run(|trx, _| {
            let (root, alpha) = (&root, &alpha);
            async move {
                let nodes = root.subspace(&("ownership", "nodes"));
                let entries = trx
                    .get_range(&foundationdb::RangeOption::from(nodes.range()), 1, true)
                    .await?;
                Ok(entries
                    .iter()
                    .map(|kv| nodes.unpack::<String>(kv.key()).unwrap())
                    .find(|n| n != alpha)
                    .unwrap())
            }
        })
        .await
        .unwrap();
    let live = vec![alpha.clone(), beta.clone()];
    let space_a = choose_space(&alpha, &live);
    let space_b = choose_space(&beta, &live);
    wait_owner(&db, &root, &space_b, &beta).await;
    wait_owner(&db, &root, &space_a, &alpha).await;
    let mut device_b = Device::connect(b.port, author.clone(), false).await;
    let create = |space: &str, privacy| {
        signed(
            &author,
            space,
            None,
            EventBody::SpaceCreated {
                title: "Ownership".into(),
                kind: SpaceKind::Group,
                privacy,
            },
        )
    };
    // Each ingress writes through the other process's owner RPC.
    let genesis = device_b
        .publish(create(&space_a, Privacy::Closed))
        .await
        .unwrap();
    device_a
        .publish(create(&space_b, Privacy::EndToEnd))
        .await
        .unwrap();
    let seen = Seen {
        seq: genesis.seq,
        hash: genesis.hash,
    };
    let before = current(&db, &root, &space_a).await;
    tokio::time::sleep(Duration::from_millis(4500)).await;
    let renewed = current(&db, &root, &space_a).await;
    assert_eq!(renewed.fence, before.fence);
    assert!(renewed.expires_version > before.expires_version);
    println!("ok  live owners renew across multiple lease periods and forward both ways");
    let bad = message(
        &author,
        &space_b,
        &Seen {
            seq: 0,
            hash: current_hash(&db, &root, &space_b).await,
        },
        "plaintext",
    );
    assert!(device_a
        .publish(bad)
        .await
        .unwrap_err()
        .contains("end-to-end"));
    println!("ok  forwarded writes preserve MLS admission");

    let pool = PgPool::connect(&url).await.unwrap();
    let target =
        forwarding::audience(&db, &root, &mut device_b, b.port, &author, &alpha, &live).await;
    forwarding::deadlines(&url, &author, &target).await;
    forwarding::public_lookup(&url, &db, &root, b.port, &target, &beta, &live).await;
    forwarding::late_delivery(
        &url,
        &db_name,
        &mut device_b,
        b.port,
        &author,
        &target,
        &alpha,
        &live,
    )
    .await;
    let stale = Arc::new(
        FdbLog::open_as(Some(&cluster), &db_name, &alpha, Duration::from_secs(1)).unwrap(),
    );
    let ingress = Arc::new(
        FdbLog::open_as(
            Some(&cluster),
            &db_name,
            "bounded-probe",
            Duration::from_secs(1),
        )
        .unwrap(),
    );
    let forwarder = Forwarder::connect(
        &ingress,
        &std::env::var("ZOEN_NATS_URL").unwrap(),
        &db_name,
        pool.clone(),
    )
    .await
    .unwrap();
    let oversized_space = "x".repeat(MAX_BYTES / 6 + 1);
    let oversized_invite = forwarder
        .invite(
            &author.identity,
            &oversized_space,
            Role::Member,
            1,
            60,
            "oversized-code",
            &renewed.fence,
        )
        .await
        .unwrap_err();
    assert!(oversized_invite.contains("capacity"));
    let invite = device_b.invite(&space_a).await;
    let replayed_invite = forwarder
        .invite(
            &author.identity,
            &space_a,
            Role::Member,
            1,
            60,
            &invite.code,
            &renewed.fence,
        )
        .await
        .unwrap();
    assert_eq!(
        (invite.code, invite.expires_at_ms),
        (replayed_invite.code, replayed_invite.expires_at_ms)
    );
    println!("ok  forwarded invite retry preserves one capability and its expiry");
    proxy.stop().await;
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if reqwest::get(format!("http://127.0.0.1:{}/readyz", a.port))
                .await
                .unwrap()
                .status()
                .as_u16()
                == 503
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("isolated owner becomes unready");
    let network_takeover = wait_owner(&db, &root, &space_a, &beta).await;
    assert!(network_takeover.fence.token > renewed.fence.token);
    let r = device_a
        .publish(message(
            &author,
            &space_a,
            &seen,
            "isolated ingress must refuse",
        ))
        .await
        .unwrap_err();
    assert!(r.contains("forwarding unavailable"));
    device_b
        .publish(message(&author, &space_a, &seen, "after NATS partition"))
        .await
        .unwrap();
    assert!(stale
        .append_fenced(
            &message(&author, &space_a, &seen, "isolated stale"),
            true,
            renewed.fence.clone()
        )
        .await
        .is_err());
    let proxy = NatsProxy::start(proxy_port, upstream).await;
    let renewed = wait_owner(&db, &root, &space_a, &alpha).await;
    assert!(renewed.fence.token > network_takeover.fence.token);
    println!(
        "ok  independent NATS partition stops renewal, changes readiness, and safely hands over"
    );
    a.signal("-STOP");
    let timed = Instant::now();
    let env = message(&author, &space_a, &seen, "never committed by paused owner");
    let replies = futures_util::future::join_all(
        (0..MAX_REQUESTS * 2).map(|_| forwarder.append(&env, &renewed.fence)),
    )
    .await;
    assert!(timed.elapsed() < Duration::from_secs(6));
    assert!(replies
        .iter()
        .all(|r| r.as_ref().err().is_some_and(|r| !r.permanent)));
    assert!(
        replies
            .iter()
            .filter(|r| r
                .as_ref()
                .err()
                .is_some_and(|r| r.reason.contains("capacity")))
            .count()
            >= MAX_REQUESTS
    );
    println!(
        "ok  paused destination bounds forwarding to 64 requests and returns retryable errors"
    );
    let takeover = wait_owner(&db, &root, &space_a, &beta).await;
    assert!(takeover.fence.token > renewed.fence.token);
    let replay = message(&author, &space_a, &seen, "survives takeover");
    let first = device_b.publish(replay.clone()).await.unwrap();
    let duplicate = device_b.publish(replay.clone()).await.unwrap();
    assert_eq!((first.seq, &first.hash), (duplicate.seq, &duplicate.hash));
    for env in [
        replay.clone(),
        message(&author, &space_a, &seen, "stale must not land"),
    ] {
        let r = stale
            .append_fenced(&env, true, renewed.fence.clone())
            .await
            .unwrap_err();
        assert!(!r.permanent && r.reason == "stale partition owner");
    }
    assert_eq!(
        stale
            .create_invite_fenced(
                &author.identity,
                &space_a,
                Role::Member,
                1,
                60,
                "stale-code",
                renewed.fence.clone()
            )
            .await
            .unwrap_err(),
        "stale partition owner"
    );
    println!("ok  expired owner rejects fresh writes, replay acknowledgments and invite minting");
    a.signal("-CONT");
    let returned = wait_owner(&db, &root, &space_a, &alpha).await;
    assert!(returned.fence.token > takeover.fence.token);
    let after_return = device_b.publish(replay.clone()).await.unwrap();
    assert_eq!(
        (after_return.seq, &after_return.hash),
        (first.seq, &first.hash)
    );
    a.stop();
    let after_death = wait_owner(&db, &root, &space_a, &beta).await;
    assert!(after_death.fence.token > returned.fence.token);
    let death = device_b
        .publish(message(&author, &space_a, &seen, "after owner death"))
        .await
        .unwrap();
    assert_eq!(death.seq, first.seq + 1);
    let events = ingress.read(&space_a, 0, 100).await.unwrap();
    assert_eq!(events.len(), 4);
    assert_eq!(
        events
            .iter()
            .filter(|ev| ev.env.client_id() == replay.client_id())
            .count(),
        1
    );
    assert_eq!(events[0].seq, 0);
    for pair in events.windows(2) {
        assert_eq!(pair[1].seq, pair[0].seq + 1);
        assert_eq!(pair[1].prev, pair[0].hash);
    }
    println!("ok  process recovery and death retain one gapless log with one copy per client_id");
    // A request directly injected on the private bus still needs active enrollment.
    sqlx::query("UPDATE devices SET revoked_at = now() WHERE device = $1")
        .bind(author.device.as_deref().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let denied = forwarder
        .append(
            &message(&author, &space_a, &seen, "revoked"),
            &after_death.fence,
        )
        .await
        .unwrap_err();
    assert!(denied.permanent && denied.reason.contains("not authorized"));
    assert_eq!(ingress.read(&space_a, 0, 100).await.unwrap().len(), 4);
    println!("ok  forwarding receiver independently refuses revoked devices");
    b.stop();
    proxy.stop().await;
    drop(forwarder);
    ingress.drop_cell().await.unwrap();
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {db_name} WITH (FORCE)"
    )))
    .execute(&mut admin)
    .await
    .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
