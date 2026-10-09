//! Real relay processes, sockets, Postgres, FoundationDB and NATS. Run with the
//! same service environment as log_store. This is correctness evidence, not load proof.

#[path = "ownership/forwarding.rs"]
mod forwarding;
#[path = "ownership/registry.rs"]
mod registry;

use foundationdb::{
    tuple::{pack, Subspace},
    Database,
};
use futures_util::{SinkExt, StreamExt};
use roda_log::{Author, Signer};
use roda_proto::{
    auth_message, ClientFrame, Envelope, InviteCreated, Op, Reply, Sequenced, ServerFrame,
    PROTOCOL_VERSION,
};
use roda_types::{EventBody, Identity, IdentityKind, Privacy, Role, Seen, SpaceKind};
use sqlx::{Connection, PgConnection, PgPool};
use std::{
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use zoen_relay::{
    log::{fdb::FdbLog, LogStore},
    ownership::{
        self,
        forward::{Forwarder, MAX_BYTES, MAX_REQUESTS},
        Lease,
    },
};

struct Node {
    child: Child,
    port: u16,
}
impl Node {
    async fn start(
        database: &str,
        cell: &str,
        dir: &std::path::Path,
        name: &str,
        nats: &str,
    ) -> Self {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let output = std::fs::File::create(dir.join(format!("{name}.log"))).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_zoen-relay"))
            .args(["--bind", &format!("127.0.0.1:{port}")])
            .env("DATABASE_URL", database)
            .env("ZOEN_FDB_CELL", cell)
            .env("ZOEN_BLOB_DIR", dir.join("blobs"))
            .env("ZOEN_OWNER_LEASE_MS", "2000")
            .env("ZOEN_NATS_URL", nats)
            .env("ZOEN_DB_POOL", "8")
            .env("RUST_LOG", "zoen_relay=debug,async_nats=info")
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .unwrap();
        let node = Self { child, port };
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if tokio::net::TcpStream::connect(("127.0.0.1", port))
                    .await
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "relay startup failed: {}",
                std::fs::read_to_string(dir.join(format!("{name}.log"))).unwrap()
            )
        });
        node
    }
    fn signal(&self, signal: &str) {
        assert!(Command::new("kill")
            .args([signal, &self.child.id().to_string()])
            .status()
            .unwrap()
            .success());
    }
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.stop();
    }
}

// The proxy isolates only A's NATS links. FoundationDB and its WebSocket
// remain reachable, so this exercises a network partition rather than a dead process.
struct NatsProxy {
    port: u16,
    task: tokio::task::JoinHandle<()>,
}
impl NatsProxy {
    async fn start(port: u16, upstream: String) -> Self {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let mut links = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut local, _) = accepted.unwrap();
                        let upstream = upstream.clone();
                        links.spawn(async move {
                            let mut remote = tokio::net::TcpStream::connect(upstream).await.unwrap();
                            let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
                        });
                    }
                    _ = links.join_next(), if !links.is_empty() => {}
                }
            }
        });
        Self { port, task }
    }
    fn url(&self) -> String {
        format!("nats://127.0.0.1:{}", self.port)
    }
    async fn stop(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
    }
}

impl Drop for NatsProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
struct Device {
    author: Author,
    socket: Socket,
}
impl Device {
    async fn connect(port: u16, author: Author, register: bool) -> Self {
        let (socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/v1/sync"))
                .await
                .unwrap();
        let mut d = Self { author, socket };
        d.send(ClientFrame::Hello {
            protocol: PROTOCOL_VERSION,
            capabilities: vec![],
            identity: d.author.identity.clone(),
            device: d.author.device.clone().unwrap(),
            cert: d.author.cert.clone().unwrap(),
        })
        .await;
        let ServerFrame::Challenge { nonce, relay, .. } = d.recv().await else {
            panic!("challenge")
        };
        d.send(ClientFrame::Auth {
            sig: d.author.key.sign(&auth_message(&nonce, &relay)),
        })
        .await;
        assert!(matches!(d.recv().await, ServerFrame::Ready { .. }));
        if register {
            let profile = Identity {
                id: d.author.identity.clone(),
                kind: IdentityKind::Person,
                name: "Ownership journey".into(),
                handle: format!("own{}", &d.author.identity[..8]),
                tint_hex: "#123456".into(),
                glyph: None,
                owner: None,
                bio: String::new(),
            };
            d.send(ClientFrame::Req {
                id: 1,
                op: Op::Register { profile },
            })
            .await;
            loop {
                if let ServerFrame::Res { id: 1, result } = d.recv().await {
                    result.unwrap();
                    break;
                }
            }
        }
        d
    }
    async fn send(&mut self, frame: ClientFrame) {
        self.socket
            .send(Message::Binary(frame.encode().into()))
            .await
            .unwrap();
    }
    async fn recv(&mut self) -> ServerFrame {
        tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                match self
                    .socket
                    .next()
                    .await
                    .expect("socket open")
                    .expect("socket frame")
                {
                    Message::Binary(bytes) => return ServerFrame::decode(&bytes).unwrap(),
                    Message::Close(_) => panic!("closed"),
                    _ => {}
                }
            }
        })
        .await
        .expect("relay answered")
    }
    async fn invite(&mut self, space: &str) -> InviteCreated {
        self.send(ClientFrame::Req {
            id: 2,
            op: Op::CreateInvite {
                space: space.into(),
                role: Role::Member,
                max_uses: 1,
                ttl_secs: 60,
            },
        })
        .await;
        loop {
            if let ServerFrame::Res { id: 2, result } = self.recv().await {
                let Reply::Invite(invite) = result.unwrap() else {
                    panic!("invite reply")
                };
                return invite;
            }
        }
    }

    async fn publish(&mut self, env: Envelope) -> Result<Sequenced, String> {
        let cid = env.client_id().to_string();
        self.send(ClientFrame::Publish { env }).await;
        loop {
            match self.recv().await {
                ServerFrame::Event { ev } if ev.env.client_id() == cid => return Ok(ev),
                ServerFrame::Rejected {
                    client_id, reason, ..
                } if client_id == cid => return Err(reason),
                _ => {}
            }
        }
    }
}

fn signed(author: &Author, space: &str, seen: Option<Seen>, body: EventBody) -> Envelope {
    Envelope::plain(&author.sign_event(space, &roda_types::new_id("cid"), 0, seen, body))
}
fn message(author: &Author, space: &str, seen: &Seen, text: &str) -> Envelope {
    signed(
        author,
        space,
        Some(seen.clone()),
        EventBody::MessagePosted {
            message: roda_types::new_id("m"),
            text: text.into(),
            attaches: None,
            reply: None,
        },
    )
}
async fn current(db: &Database, root: &Subspace, space: &str) -> Lease {
    db.run(|trx, _| async move {
        ownership::bound_transaction(&trx)?;
        Ok(
            ownership::lease_in(&trx, root, ownership::partition_of(space))
                .await?
                .unwrap(),
        )
    })
    .await
    .unwrap()
}
async fn wait_owner(db: &Database, root: &Subspace, space: &str, owner: &str) -> Lease {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let l = current(db, root, space).await;
            if l.fence.owner == owner {
                return l;
            }
            tokio::time::sleep(Duration::from_millis(75)).await;
        }
    })
    .await
    .expect("ownership converged")
}
fn choose_space(preferred: &str, live: &[String]) -> String {
    let owned = ownership::owned_partitions(preferred, live);
    loop {
        let space = roda_types::new_id("sp");
        if owned.contains(&ownership::partition_of(&space)) {
            return space;
        }
    }
}

async fn journey() {
    let admin_url = std::env::var("ZOEN_TEST_PG").expect("Postgres");
    let db_name = format!("ownership_{}", roda_types::new_id("t").to_lowercase());
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
    let probe = roda_types::new_id("sp");
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

async fn current_hash(db: &Database, root: &Subspace, space: &str) -> String {
    db.run(|trx, _| async move {
        let head = trx
            .get(&root.pack(&("s", space, "head")), false)
            .await?
            .unwrap();
        let (_, hash): (i64, String) = foundationdb::tuple::unpack(&head).unwrap();
        Ok(hash)
    })
    .await
    .unwrap()
}

async fn transaction_conflict() {
    let cell = roda_types::new_id("t");
    let cluster = std::env::var("FDB_CLUSTER_FILE").unwrap();
    let db = Arc::new(Database::new(Some(&cluster)).unwrap());
    let root = Subspace::all().subspace(&("zoen", &cell));
    let a = ownership::NodeOwner::new(
        db.clone(),
        root.clone(),
        "alpha".into(),
        Duration::from_secs(30),
    );
    let b = ownership::NodeOwner::new(
        db.clone(),
        root.clone(),
        "beta".into(),
        Duration::from_secs(30),
    );
    let space = choose_space("beta", &["alpha".into(), "beta".into()]);
    a.maintain().await.unwrap();
    let initial = current(&db, &root, &space).await;
    let old = db.create_trx().unwrap();
    let lease = ownership::lease_in(&old, &root, ownership::partition_of(&space))
        .await
        .unwrap();
    assert!(ownership::accepts(
        lease.as_ref(),
        &initial.fence,
        &space,
        old.get_read_version().await.unwrap()
    ));
    old.set(&root.pack(&("proof", "stale-write")), &pack(&1i64));
    // The process journey above proves natural expiry. Here expire only the fixture
    // lease in the store, keeping the admitted transaction younger than FDB's 5s
    // transaction window even when its version clock advances slowly.
    db.run(|trx, _| {
        let (root, initial) = (&root, &initial);
        async move {
            let version = trx.get_read_version().await?;
            let lease = ownership::lease_in(&trx, root, initial.fence.partition)
                .await?
                .unwrap();
            assert_eq!(lease.fence, initial.fence);
            trx.set(
                &ownership::lease_key(root, lease.fence.partition),
                &pack(&(lease.fence.owner.as_str(), lease.fence.token, version)),
            );
            Ok(())
        }
    })
    .await
    .unwrap();
    b.maintain().await.unwrap();
    let next = current(&db, &root, &space).await;
    assert_eq!(next.fence.owner, "beta");
    assert!(next.fence.token > initial.fence.token);
    match old.commit().await {
        Err(e) => assert_eq!(
            e.code(),
            1020,
            "takeover must cause a read conflict, not just transaction age"
        ),
        Ok(_) => panic!("a transaction admitted before takeover must conflict"),
    }
    let stale = root.pack(&("proof", "stale-write"));
    assert!(db
        .run(|trx, _| {
            let stale = &stale;
            async move { Ok(trx.get(stale, false).await?) }
        })
        .await
        .unwrap()
        .is_none());
    let (begin, end) = root.range();
    db.run(|trx, _| {
        let (begin, end) = (&begin, &end);
        async move {
            trx.clear_range(begin, end);
            Ok(())
        }
    })
    .await
    .unwrap();
    println!(
        "ok  a transaction admitted before takeover cannot commit after the generation changes"
    );
}

fn main() {
    let network = unsafe { foundationdb::boot() };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        journey().await;
        transaction_conflict().await;
        registry::run().await;
    });
    drop(network);
}
