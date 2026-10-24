//! Real relay processes, sockets, Postgres, FoundationDB and NATS. Run with the
//! same service environment as log_store. This is correctness evidence, not load proof.

#[path = "ownership/forwarding.rs"]
mod forwarding;
#[path = "ownership/registry.rs"]
mod registry;

use event_log::{Author, Signer};
use foundationdb::{
    tuple::{pack, Subspace},
    Database,
};
use futures_util::{SinkExt, StreamExt};
use protocol::{
    auth_message, ClientFrame, Envelope, InviteCreated, Op, Reply, Sequenced, ServerFrame,
    PROTOCOL_VERSION,
};
use sqlx::{Connection, PgConnection, PgPool};
use std::{
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use types::{EventBody, Identity, IdentityKind, Privacy, Role, Seen, SpaceKind};
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
                owner_proof: None,
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
                agent: None,
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

fn signed(
    author: &Author,
    space: &str,
    seen: Option<Seen>,
    body: EventBody,
) -> std::result::Result<Envelope, Box<dyn std::error::Error>> {
    Ok(Envelope::plain(&author.sign_event(
        space,
        &types::new_id("cid")?,
        0,
        seen,
        body,
    )?))
}
fn message(
    author: &Author,
    space: &str,
    seen: &Seen,
    text: &str,
) -> std::result::Result<Envelope, Box<dyn std::error::Error>> {
    Ok(signed(
        author,
        space,
        Some(seen.clone()),
        EventBody::MessagePosted {
            message: types::new_id("m")?,
            text: text.into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )?)
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
fn choose_space(
    preferred: &str,
    live: &[String],
) -> std::result::Result<String, Box<dyn std::error::Error>> {
    let owned = ownership::owned_partitions(preferred, live).unwrap();
    loop {
        let space = types::new_id("sp")?;
        if owned.contains(&ownership::partition_of(&space)) {
            return Ok(space);
        }
    }
}

#[path = "ownership/journey.rs"]
mod journey;
use journey::journey;

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

async fn transaction_conflict() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cell = types::new_id("t")?;
    let cluster = std::env::var("FDB_CLUSTER_FILE").unwrap();
    let db = Arc::new(Database::new(Some(&cluster)).unwrap());
    let root = Subspace::all().subspace(&("zoen", &cell));
    let a = ownership::NodeOwner::new(
        db.clone(),
        root.clone(),
        "alpha".into(),
        Duration::from_secs(30),
    )
    .unwrap();
    let b = ownership::NodeOwner::new(
        db.clone(),
        root.clone(),
        "beta".into(),
        Duration::from_secs(30),
    )
    .unwrap();
    let space = choose_space("beta", &["alpha".into(), "beta".into()])?;
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
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let network = unsafe { foundationdb::boot() };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        journey().await?;
        transaction_conflict().await?;
        registry::run().await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    drop(network);
    Ok(())
}
