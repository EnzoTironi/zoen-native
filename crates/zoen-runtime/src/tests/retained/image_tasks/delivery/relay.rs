//! Actual production HTTP/blob/WebSocket handlers over the original relay log.
use super::*;
use tokio::net::TcpListener;
use zoen_relay::{AppState, Shared};

pub(super) struct Relay {
    pub(super) address: std::net::SocketAddr,
    pub(super) state: Shared,
    server: tokio::task::JoinHandle<()>,
    blobs: std::path::PathBuf,
}
impl Relay {
    pub(super) async fn start(f: &Fixture) -> Self {
        f.log.owner.maintain().await.unwrap();
        let blobs = std::env::temp_dir().join(new_id("zoen-image-355-blobs"));
        std::fs::create_dir(&blobs).unwrap();
        let store = object_store::local::LocalFileSystem::new_with_prefix(&blobs).unwrap();
        let pool = sqlx::PgPool::connect(&f.world.db_url).await.unwrap();
        let state = Arc::new(AppState {
            auth: zoen_relay::auth::Service::disabled(),
            turn: zoen_relay::turn::Service::disabled(),
            pool: pool.clone(),
            session_auth: pool.clone(),
            delivery_auth: pool,
            log: f.log.clone(),
            fanout: zoen_relay::fanout::Fanout::local(f.log.owner.node.clone()),
            limits: zoen_relay::limits::Limits::from_spec("").unwrap(),
            client_ip_header: None,
            relay_name: "image-output-real-relay-355".into(),
            metrics: Default::default(),
            blobs: Arc::new(store),
            apple_app_ids: vec![],
            android_packages: vec![],
            android_store_url: None,
            backup_vault: None,
            backup_settings: Default::default(),
            analytics: zoen_relay::analytics::Analytics::with_key([93; 32], ""),
            push: Arc::new(zoen_relay::push::Pusher::disabled()),
            receipts: Default::default(),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = zoen_relay::router(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        Self {
            address,
            state,
            server,
            blobs,
        }
    }
    pub(super) fn target(
        &self,
        f: &Fixture,
        address: std::net::SocketAddr,
    ) -> crate::ImageDeliveryTarget {
        crate::ImageDeliveryTarget::configured(
            format!("http://{address}"),
            self.state.relay_name.clone(),
            f.world.relay_cell.clone().unwrap(),
        )
        .unwrap()
    }
    pub(super) async fn stop(self) {
        self.state.pool.close().await;
    }
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.server.abort();
        std::fs::remove_dir_all(&self.blobs).unwrap();
    }
}
