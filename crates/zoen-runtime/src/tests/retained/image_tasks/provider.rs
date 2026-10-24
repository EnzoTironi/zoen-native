//! Unpaid local HTTP service reached through the actual bounded Images gateway.
use super::*;
use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::atomic::AtomicUsize;

struct Service {
    body: Value,
    sends: Arc<AtomicUsize>,
}
pub(super) struct Provider {
    pub(super) endpoint: String,
    pub(super) sends: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}
impl Provider {
    pub(super) async fn start(bytes: &[u8]) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!(
            "http://{}/v1/images/generations",
            listener.local_addr().unwrap()
        );
        let sends = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(Service {
            sends: sends.clone(),
            body: json!({"created":1,"data":[{"b64_json":STANDARD.encode(bytes)}],
                "usage":{"input_tokens":5,"output_tokens":2,"total_tokens":7,
                    "input_tokens_details":{"text_tokens":2,"image_tokens":3},
                    "output_tokens_details":{"text_tokens":1,"image_tokens":1}}}),
        });
        let app = Router::new()
            .route("/v1/images/generations", post(generate))
            .with_state(state);
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            endpoint,
            sends,
            server,
        }
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.server.abort();
    }
}
async fn generate(
    State(state): State<Arc<Service>>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Json<Value> {
    assert_eq!(
        headers.get("authorization").unwrap(),
        "Bearer controlled-image-adapter-not-a-provider-secret"
    );
    assert_eq!(request["model"], "gpt-image-fixture");
    assert_eq!(request["prompt"], "A quiet blue sky");
    assert_eq!(request["size"], "1024x1024");
    assert_eq!(request["quality"], "low");
    assert_eq!(request["background"], "opaque");
    assert_eq!(request["n"], 1);
    assert_eq!(request["output_format"], "png");
    assert!(request.get("max_output_tokens").is_none());
    state.sends.fetch_add(1, Ordering::SeqCst);
    Json(state.body.clone())
}
