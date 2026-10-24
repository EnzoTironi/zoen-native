//! Synthetic membership seam; directory admission, WS frames and SQL are real.
//! This fixture cannot append or grant membership to an identity outside its roster.
use async_trait::async_trait;
use event_log::{Author, Signer};
use futures_util::{SinkExt, StreamExt};
use protocol::{
    auth_message, ClientFrame, Envelope, InviteCreated, Op, Reply, Sequenced, ServerFrame,
    PROTOCOL_VERSION,
};
use sqlx::{Connection, PgConnection, PgPool};
use std::{sync::Arc, time::Duration};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use types::{Identity, IdentityKind, Role};
use zoen_relay::{
    analytics, fanout, limits,
    log::{InviteInfo, LogStore, Reject, Sequencing, StoreError},
    metrics::Metrics,
    receipts::Receipts,
    AppState,
};

pub const SPACE: &str = "sp_receipt_pressure";
pub type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
struct Roster(Vec<String>);
#[async_trait]
impl LogStore for Roster {
    async fn append(&self, _: &Envelope, _: bool) -> Result<Sequencing, Reject> {
        Err(Reject::unavailable())
    }
    async fn read(&self, _: &str, _: u64, _: usize) -> Result<Vec<Sequenced>, StoreError> {
        Ok(vec![])
    }
    async fn read_visible(
        &self,
        space: &str,
        who: &str,
        from: u64,
        limit: usize,
    ) -> Result<Option<zoen_relay::log::VisiblePage>, StoreError> {
        let _ = (space, who, from, limit);
        Ok(self
            .role(space, who)
            .await?
            .map(|_| zoen_relay::log::VisiblePage {
                events: vec![],
                through: None,
            }))
    }
    async fn role(&self, space: &str, who: &str) -> Result<Option<Role>, StoreError> {
        Ok((space == SPACE && self.0.iter().any(|id| id == who)).then_some(Role::Member))
    }
    async fn removed_at(&self, _: &str, _: &str) -> Result<Option<u64>, StoreError> {
        Ok(None)
    }
    async fn members(&self, space: &str) -> Result<Vec<(String, Role)>, StoreError> {
        Ok(if space == SPACE {
            self.0.iter().map(|id| (id.clone(), Role::Member)).collect()
        } else {
            vec![]
        })
    }
    async fn spaces_of(&self, who: &str) -> Result<Vec<String>, StoreError> {
        Ok(if self.0.iter().any(|id| id == who) {
            vec![SPACE.into()]
        } else {
            vec![]
        })
    }
    async fn co_members(&self, _: &str) -> Result<Vec<String>, StoreError> {
        Ok(vec![])
    }
    async fn create_invite(
        &self,
        _: &str,
        _: &str,
        _: Role,
        _: u32,
        _: u64,
    ) -> Result<InviteCreated, String> {
        Err("unused".into())
    }
    async fn create_agent_invite(
        &self,
        _: &str,
        _: &str,
        _: u32,
        _: u64,
    ) -> Result<InviteCreated, String> {
        Err("unused".into())
    }
    async fn preview_invite(&self, _: &str) -> Result<InviteInfo, String> {
        Err("unused".into())
    }
    async fn redeem_agent_invite(&self, _: &str, _: &str) -> Result<InviteInfo, String> {
        Err("unused".into())
    }
    async fn revoke_agent_invite(&self, _: &str) -> Result<(), String> {
        Err("unused".into())
    }
}

pub async fn send(socket: &mut Socket, frame: ClientFrame) {
    socket
        .send(Message::Binary(frame.encode().into()))
        .await
        .unwrap();
}
pub async fn recv(socket: &mut Socket) -> Option<ServerFrame> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("socket stalled")
        {
            Some(Ok(Message::Binary(bytes))) => return Some(ServerFrame::decode(&bytes).unwrap()),
            None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return None,
            _ => {}
        }
    }
}
pub async fn barrier(socket: &mut Socket) {
    send(socket, ClientFrame::Ping).await;
    assert!(matches!(recv(socket).await, Some(ServerFrame::Pong { .. })));
}

pub struct Relay {
    pub state: Arc<AppState>,
    url: String,
    server: tokio::task::JoinHandle<()>,
    database: String,
    admin: PgConnection,
}
impl Relay {
    pub async fn new(roster: Vec<String>, rows: usize) -> Self {
        let admin_url =
            std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG to an owned PostgreSQL");
        let database = format!("zoen_pressure_{}", &Signer::generate().id()[..16]);
        let mut admin = PgConnection::connect(&admin_url).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&mut admin)
            .await
            .unwrap();
        let options = admin_url
            .parse::<sqlx::postgres::PgConnectOptions>()
            .unwrap()
            .database(&database);
        let pool = PgPool::connect_with(options.clone()).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let state = Arc::new(AppState {
            read_snapshots: zoen_relay::read_snapshots::Resources::new(&pool),
            auth: zoen_relay::auth::Service::disabled(),
            turn: zoen_relay::turn::Service::disabled(),
            analytics: analytics::Analytics::load(&pool).await.unwrap(),
            pool,
            session_auth: PgPool::connect_with(options.clone()).await.unwrap(),
            delivery_auth: PgPool::connect_with(options).await.unwrap(),
            log: Arc::new(Roster(roster)),
            fanout: fanout::Fanout::local("receipt-pressure".into()),
            limits: limits::Limits::from_spec("").unwrap(),
            client_ip_header: None,
            relay_name: "receipt-pressure".into(),
            metrics: Metrics::default(),
            blobs: Arc::new(object_store::memory::InMemory::new()),
            apple_app_ids: vec![],
            android_packages: vec![],
            android_store_url: None,
            backup_vault: None,
            backup_settings: Default::default(),
            push: Arc::new(zoen_relay::push::Pusher::disabled()),
            receipts: Receipts::with_capacity(rows, rows * (SPACE.len() + 64)),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = zoen_relay::router(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap()
        });
        Self {
            state,
            url: format!("ws://{address}/v1/sync"),
            server,
            database,
            admin,
        }
    }
    pub async fn connect(&self, author: &Author, register: bool) -> Socket {
        let (mut socket, _) = tokio_tungstenite::connect_async(&self.url).await.unwrap();
        send(
            &mut socket,
            ClientFrame::Hello {
                protocol: PROTOCOL_VERSION,
                capabilities: vec![],
                identity: author.identity.clone(),
                device: author.device.clone().unwrap(),
                cert: author.cert.clone().unwrap(),
            },
        )
        .await;
        let Some(ServerFrame::Challenge { nonce, relay, .. }) = recv(&mut socket).await else {
            panic!("challenge")
        };
        assert_eq!(relay, "receipt-pressure");
        send(
            &mut socket,
            ClientFrame::Auth {
                sig: author.key.sign(&auth_message(&nonce, &relay)),
            },
        )
        .await;
        assert!(matches!(
            recv(&mut socket).await,
            Some(ServerFrame::Ready { .. })
        ));
        if register {
            send(
                &mut socket,
                ClientFrame::Req {
                    id: 1,
                    op: Op::Register {
                        profile: Identity {
                            id: author.identity.clone(),
                            kind: IdentityKind::Person,
                            name: "Receipt test".into(),
                            handle: format!("pressure_{}", &author.identity[..12]),
                            tint_hex: "#123456".into(),
                            glyph: None,
                            owner: None,
                            owner_proof: None,
                            bio: String::new(),
                        },
                    },
                },
            )
            .await;
            assert!(matches!(
                recv(&mut socket).await,
                Some(ServerFrame::Res {
                    id: 1,
                    result: Ok(Reply::Registered(_))
                })
            ));
        }
        barrier(&mut socket).await;
        socket
    }
    pub async fn close(mut self) {
        self.server.abort();
        let _ = self.server.await;
        self.state.pool.close().await;
        self.state.session_auth.close().await;
        self.state.delivery_auth.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.database
        )))
        .execute(&mut self.admin)
        .await
        .unwrap();
    }
}
