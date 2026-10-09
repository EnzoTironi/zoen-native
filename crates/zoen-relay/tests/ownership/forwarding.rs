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
        matches!(receiver.append(&uncertain, &fence).await.unwrap(), Sequencing::Duplicate { ev } if ev.seq == 1)
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
