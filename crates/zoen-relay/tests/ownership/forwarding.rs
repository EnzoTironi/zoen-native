use super::*;
use sha2::{Digest, Sha256};
use zoen_relay::{
    log::Sequencing,
    ownership::forward::{RECEIVER_TIMEOUT, RPC_TIMEOUT},
};

pub async fn audience(
    db: &Database,
    root: &Subspace,
    ingress: &mut Device,
    ingress_port: u16,
    author: &Author,
    owner: &str,
    live: &[String],
) -> Author {
    let space = choose_space(owner, live);
    let genesis = ingress
        .publish(signed(
            author,
            &space,
            None,
            EventBody::SpaceCreated {
                title: "Large audience fixture".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ))
        .await
        .unwrap();
    let seen = Seen {
        seq: genesis.seq,
        hash: genesis.hash.clone(),
    };
    // Seed the existing accepted shape under the head fence. Include its signed
    // membership history so invalidating the owner's cache retains a valid chain.
    let mut previous = genesis.hash;
    let mut additions = Vec::new();
    for i in 1..4_000 {
        let identity = format!("{i:064x}");
        let env = signed(
            author,
            &space,
            Some(seen.clone()),
            EventBody::MemberAdded {
                identity: identity.clone(),
                role: Role::Member,
            },
        );
        let hash = roda_log::chain_hash(&space, i, &previous, &env.wire_hash());
        let ev = Sequenced {
            seq: i,
            prev: previous,
            hash: hash.clone(),
            env,
        };
        additions.push((identity, ev));
        previous = hash;
    }
    db.run(|trx, _| {
        let (root, space, additions, previous) = (root, &space, &additions, &previous);
        async move {
            let keys = root.subspace(&("s", space));
            trx.get(&keys.pack(&"head"), false).await?;
            for (identity, ev) in additions {
                trx.set(&keys.pack(&("m", identity.as_str())), &pack(&"Member"));
                trx.set(&keys.pack(&("log", ev.seq as i64)), &ev.encode());
                trx.set(
                    &keys.pack(&("dedupe", ev.env.author(), ev.env.client_id())),
                    &pack(&(ev.seq as i64)),
                );
            }
            trx.set(&keys.pack(&"head"), &pack(&(3_999i64, previous.as_str())));
            Ok(())
        }
    })
    .await
    .unwrap();
    let target = Author::device(&Signer::generate(), Signer::generate());
    let mut recipient = Device::connect(ingress_port, target.clone(), true).await;
    let add = signed(
        author,
        &space,
        Some(seen),
        EventBody::MemberAdded {
            identity: target.identity.clone(),
            role: Role::Member,
        },
    );
    let cid = add.client_id().to_string();
    assert_eq!(ingress.publish(add).await.unwrap().seq, 4_000);
    let (mut event, mut joined) = (false, false);
    while !event || !joined {
        match recipient.recv().await {
            ServerFrame::Event { ev } if ev.env.client_id() == cid => event = true,
            ServerFrame::Joined { space: got } if got == space => joined = true,
            _ => {}
        }
    }
    println!("ok  a seeded 4000-member owner response preserves New live delivery and Joined");
    target
}

fn headers(fence: &ownership::Fence) -> async_nats::HeaderMap {
    let mut headers = async_nats::HeaderMap::new();
    let request = serde_json::json!({"fence": fence, "invite": null}).to_string();
    headers.insert("Zoen-Owner-Fence", request.as_str());
    headers
}

async fn wait_slots(receiver: &Forwarder, slots: usize) {
    tokio::time::timeout(RECEIVER_TIMEOUT + Duration::from_secs(2), async {
        while receiver.available_requests() != slots {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("receiver capacity recovered within its deadline");
}

pub async fn deadlines(database: &str, author: &Author, target: &Author) {
    let cluster = std::env::var("FDB_CLUSTER_FILE").unwrap();
    let cell = roda_types::new_id("forward-deadline");
    let log = Arc::new(
        FdbLog::open_as(
            Some(&cluster),
            &cell,
            "deadline-owner",
            Duration::from_secs(60),
        )
        .unwrap(),
    );
    log.owner.maintain().await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(database)
        .await
        .unwrap();
    let nats = std::env::var("ZOEN_NATS_URL").unwrap();
    let receiver = Forwarder::connect(&log, &nats, &cell, pool.clone())
        .await
        .unwrap();
    let client = async_nats::ConnectOptions::new()
        .request_timeout(Some(RPC_TIMEOUT))
        .connect(&nats)
        .await
        .unwrap();
    let subject = format!(
        "zoen.{}.owner.{}",
        hex::encode(Sha256::digest(cell.as_bytes())),
        hex::encode(Sha256::digest(b"deadline-owner"))
    );
    let space = roda_types::new_id("sp");
    let db = Database::new(Some(&cluster)).unwrap();
    let root = Subspace::all().subspace(&("zoen", &cell));
    let fence = current(&db, &root, &space).await.fence;
    let create = signed(
        author,
        &space,
        None,
        EventBody::SpaceCreated {
            title: "Deadlines".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    );
    let Sequencing::New { ev, .. } = log
        .append_fenced(&create, true, fence.clone())
        .await
        .unwrap()
    else {
        panic!("new Space")
    };
    let seen = Seen {
        seq: ev.seq,
        hash: ev.hash,
    };

    // The caller times out before PostgreSQL releases admission. Its durable CID
    // must commit once, then replay as Duplicate after the original inbox is gone.
    let mut lock = PgConnection::connect(database).await.unwrap();
    sqlx::query("BEGIN").execute(&mut lock).await.unwrap();
    sqlx::query("SELECT 1 FROM devices WHERE device = $1 FOR UPDATE")
        .bind(author.device.as_deref().unwrap())
        .execute(&mut lock)
        .await
        .unwrap();
    let uncertain = signed(
        author,
        &space,
        Some(seen.clone()),
        EventBody::MemberAdded {
            identity: target.identity.clone(),
            role: Role::Member,
        },
    );
    let wire = ClientFrame::Publish {
        env: uncertain.clone(),
    }
    .encode();
    let pending = tokio::spawn({
        let (client, subject, headers) = (client.clone(), subject.clone(), headers(&fence));
        async move {
            client
                .request_with_headers(subject, headers, wire.into())
                .await
        }
    });
    wait_slots(&receiver, MAX_REQUESTS - 1).await;
    assert!(pending.await.unwrap().is_err());
    assert_eq!(
        receiver.available_requests(),
        MAX_REQUESTS - 1,
        "receiver still owns the blocked operation"
    );
    sqlx::query("COMMIT").execute(&mut lock).await.unwrap();
    wait_slots(&receiver, MAX_REQUESTS).await;
    assert!(
        matches!(receiver.append(&uncertain, &fence).await.unwrap(), Sequencing::Duplicate { ev, .. } if ev.seq == 1)
    );
    assert_eq!(log.read(&space, 0, 100).await.unwrap().len(), 2);
    println!(
        "ok  a forwarded publish with an unknown caller outcome commits one CID and replays once"
    );

    sqlx::query("BEGIN").execute(&mut lock).await.unwrap();
    sqlx::query("LOCK TABLE identities IN ACCESS EXCLUSIVE MODE")
        .execute(&mut lock)
        .await
        .unwrap();
    let timed = Instant::now();
    let mut requests = Vec::new();
    for batch in 0..MAX_REQUESTS / 8 {
        for _ in 0..8 {
            let (client, subject, headers) = (client.clone(), subject.clone(), headers(&fence));
            let env = signed(
                author,
                &space,
                Some(seen.clone()),
                EventBody::MemberAdded {
                    identity: target.identity.clone(),
                    role: Role::Member,
                },
            );
            let wire = ClientFrame::Publish { env }.encode();
            requests.push(tokio::spawn(async move {
                client
                    .request_with_headers(subject, headers, wire.into())
                    .await
            }));
        }
        // Pace the producer against admitted work rather than overflowing the
        // deliberately bounded 32-message NATS subscription used in production.
        wait_slots(&receiver, MAX_REQUESTS - (batch + 1) * 8).await;
    }
    // Causal control: all eight SQL connections are blocked on the identities
    // lookup, with the remaining receiver tasks waiting for a pool connection.
    let blocked = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE 'SELECT EXISTS(SELECT 1 FROM identities%'")
                .fetch_one(&mut lock).await.unwrap();
            if n == 8 { break n }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("all SQL lookup connections reached the injected lock");
    assert_eq!(blocked, 8);
    for request in requests {
        let _ = request.await.unwrap();
    }
    wait_slots(&receiver, MAX_REQUESTS).await;
    assert!(timed.elapsed() < RECEIVER_TIMEOUT + Duration::from_secs(2));
    assert!(
        matches!(receiver.append(&message(author, &space, &seen, "capacity recovered while lookup is locked"), &fence).await.unwrap(), Sequencing::New { ev, .. } if ev.seq == 2)
    );
    assert_eq!(
        log.read(&space, 0, 100).await.unwrap().len(),
        3,
        "timed-out locked lookups never entered the durable queue"
    );
    sqlx::query("COMMIT").execute(&mut lock).await.unwrap();
    drop(receiver);
    log.drop_cell().await.unwrap();
    pool.close().await;
    println!("ok  blocked PostgreSQL lookups and pool waits release all 64 receiver slots within the whole-operation deadline");
}

pub async fn public_lookup(
    database: &str,
    db: &Database,
    root: &Subspace,
    port: u16,
    target: &Author,
    owner: &str,
    live: &[String],
) {
    let author = Author::device(&Signer::generate(), Signer::generate());
    let mut ingress = Device::connect(port, author.clone(), true).await;
    let space = choose_space(owner, live);
    let genesis = ingress
        .publish(signed(
            &author,
            &space,
            None,
            EventBody::SpaceCreated {
                title: "Public directory deadline".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ))
        .await
        .unwrap();
    let seen = Seen {
        seq: genesis.seq,
        hash: genesis.hash,
    };
    let mut sockets = Vec::new();
    for _ in 0..8 {
        let mut socket = Device::connect(port, author.clone(), false).await;
        // Ready precedes go_online. Drain that startup work before occupying the
        // directory pool so every blocked lookup belongs to the injected publish.
        socket.send(ClientFrame::Ping).await;
        while !matches!(socket.recv().await, ServerFrame::Pong { .. }) {}
        sockets.push(socket);
    }
    let mut lock = PgConnection::connect(database).await.unwrap();
    sqlx::query("BEGIN").execute(&mut lock).await.unwrap();
    sqlx::query("LOCK TABLE identities IN ACCESS EXCLUSIVE MODE")
        .execute(&mut lock)
        .await
        .unwrap();
    let mut ids = Vec::new();
    for socket in &mut sockets {
        let env = signed(
            &author,
            &space,
            Some(seen.clone()),
            EventBody::MemberAdded {
                identity: target.identity.clone(),
                role: Role::Member,
            },
        );
        ids.push(env.client_id().to_string());
        socket.send(ClientFrame::Publish { env }).await;
    }
    let mut most_blocked = 0;
    let established = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            sqlx::query("SELECT pg_stat_clear_snapshot()")
                .execute(&mut lock).await.unwrap();
            let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE 'SELECT EXISTS(SELECT 1 FROM identities%'")
                .fetch_one(&mut lock).await.unwrap();
            most_blocked = most_blocked.max(n);
            if n == 8 { break }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await;
    if established.is_err() {
        let blocked = sqlx::query_as::<_, (String, Option<String>)>("SELECT query, wait_event FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock'")
            .fetch_all(&mut lock).await.unwrap();
        panic!("eight real WebSocket publishes reached the directory lock: maximum {most_blocked}, blocked {blocked:?}");
    }
    let timed = Instant::now();
    let healthy = ingress
        .publish(message(
            &author,
            &space,
            &seen,
            "ordinary messages recover while directory lookup is locked",
        ))
        .await
        .unwrap();
    assert_eq!(healthy.seq, 1);
    assert!(timed.elapsed() < Duration::from_secs(3));
    for (socket, cid) in sockets.iter_mut().zip(ids) {
        loop {
            if let ServerFrame::Rejected {
                client_id,
                reason,
                permanent,
                ..
            } = socket.recv().await
            {
                if client_id == cid {
                    assert!(!permanent && reason == "directory unavailable");
                    break;
                }
            }
        }
    }
    assert_eq!(current_hash(db, root, &space).await, healthy.hash);
    sqlx::query("COMMIT").execute(&mut lock).await.unwrap();
    println!("ok  real WebSocket directory lookups fail retryably and ordinary messages recover while the identities table remains locked");
}

#[allow(clippy::too_many_arguments)]
pub async fn late_delivery(
    database: &str,
    cell: &str,
    ingress: &mut Device,
    port: u16,
    author: &Author,
    target: &Author,
    owner: &str,
    live: &[String],
) {
    let cluster = std::env::var("FDB_CLUSTER_FILE").unwrap();
    let log = FdbLog::open_as(
        Some(&cluster),
        cell,
        "delivery-read-probe",
        Duration::from_secs(60),
    )
    .unwrap();
    let db = Database::new(Some(&cluster)).unwrap();
    let root = Subspace::all().subspace(&("zoen", cell));
    let space = choose_space(owner, live);
    let genesis = ingress
        .publish(signed(
            author,
            &space,
            None,
            EventBody::SpaceCreated {
                title: "Unknown outcome delivery".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ))
        .await
        .unwrap();
    let seen = Seen {
        seq: genesis.seq,
        hash: genesis.hash,
    };
    let fence = current(&db, &root, &space).await.fence;
    let mut recipient = Device::connect(port, target.clone(), false).await;
    let unknown = signed(
        author,
        &space,
        Some(seen.clone()),
        EventBody::MemberAdded {
            identity: target.identity.clone(),
            role: Role::Member,
        },
    );
    let mut lock = PgConnection::connect(database).await.unwrap();
    sqlx::query("BEGIN").execute(&mut lock).await.unwrap();
    sqlx::query("SELECT 1 FROM devices WHERE device = $1 FOR UPDATE")
        .bind(author.device.as_deref().unwrap())
        .execute(&mut lock)
        .await
        .unwrap();
    let nats = std::env::var("ZOEN_NATS_URL").unwrap();
    let client = async_nats::ConnectOptions::new()
        .request_timeout(Some(RPC_TIMEOUT))
        .connect(&nats)
        .await
        .unwrap();
    let subject = format!(
        "zoen.{}.owner.{}",
        hex::encode(Sha256::digest(cell.as_bytes())),
        hex::encode(Sha256::digest(owner.as_bytes()))
    );
    let request = tokio::spawn({
        let wire = ClientFrame::Publish {
            env: unknown.clone(),
        }
        .encode();
        let headers = headers(&fence);
        async move {
            client
                .request_with_headers(subject, headers, wire.into())
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            sqlx::query("SELECT pg_stat_clear_snapshot()")
                .execute(&mut lock).await.unwrap();
            let blocked = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE 'SELECT identity = $1,%'")
                .fetch_one(&mut lock).await.unwrap();
            if blocked >= 1 { break }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("the receiving owner's device admission reached the injected lock");
    assert!(
        request.await.unwrap().is_err(),
        "the caller inbox expires before the owner commits"
    );
    sqlx::query("COMMIT").execute(&mut lock).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while log.read(&space, 0, 100).await.unwrap().len() != 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let replay = ingress.publish(unknown.clone()).await.unwrap();
    assert_eq!(replay.seq, 1);
    let (mut event, mut joined) = (false, false);
    while !event || !joined {
        match recipient.recv().await {
            ServerFrame::Event { ev } if ev.env.client_id() == unknown.client_id() => event = true,
            ServerFrame::Joined { space: got } if got == space => joined = true,
            _ => {}
        }
    }
    let remove = signed(
        author,
        &space,
        Some(seen),
        EventBody::MemberRemoved {
            identity: target.identity.clone(),
        },
    );
    let removal_id = remove.client_id().to_string();
    assert_eq!(ingress.publish(remove).await.unwrap().seq, 2);
    loop {
        if let ServerFrame::Event { ev } = recipient.recv().await {
            if ev.env.client_id() == removal_id {
                break;
            }
        }
    }
    assert_eq!(ingress.publish(unknown.clone()).await.unwrap().seq, 1);
    let forbidden_delivery = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            match recipient.recv().await {
                ServerFrame::Event { ev } if ev.env.client_id() == unknown.client_id() => break,
                ServerFrame::Joined { space: got } if got == space => break,
                _ => {}
            }
        }
    })
    .await;
    assert!(
        forbidden_delivery.is_err(),
        "duplicate replay must exclude a removed identity"
    );
    let events = log.read(&space, 0, 100).await.unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(
        events
            .iter()
            .filter(|ev| ev.env.client_id() == unknown.client_id())
            .count(),
        1
    );
    println!("ok  unknown owner outcomes replay Event and Joined to live current members with one durable CID and no replay to removed identities");
}
