use super::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const KEY: &str = "test-key-SECRET-CANARY-8b1";
const PROMPT: &str = "prompt-PRIVATE-CANARY-29a";
const BODY: &str = "reply-PRIVATE-CANARY-45c";

struct Fixture {
    base: String,
    seen: Arc<Mutex<Vec<(String, Value)>>>,
    connections: Arc<AtomicUsize>,
    received: Arc<tokio::sync::Notify>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fixture {
    async fn serve(
        status: u16,
        body: String,
        headers: &str,
        chunked: bool,
        delay: Duration,
        lost: bool,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let received = Arc::new(tokio::sync::Notify::new());
        let signal = received.clone();
        let (records, count, headers) = (seen.clone(), connections.clone(), headers.to_owned());
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                count.fetch_add(1, Ordering::SeqCst);
                let (records, body, headers, signal) = (
                    records.clone(),
                    body.clone(),
                    headers.clone(),
                    signal.clone(),
                );
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let header_end;
                    loop {
                        let mut chunk = [0; 4096];
                        let n = stream.read(&mut chunk).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        buffer.extend_from_slice(&chunk[..n]);
                        if let Some(end) = buffer.windows(4).position(|v| v == b"\r\n\r\n") {
                            header_end = end + 4;
                            break;
                        }
                        assert!(buffer.len() < 1_048_576);
                    }
                    let request_headers = String::from_utf8(buffer[..header_end].to_vec()).unwrap();
                    let length = request_headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    while buffer.len() < header_end + length {
                        let mut chunk = [0; 4096];
                        let n = stream.read(&mut chunk).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        buffer.extend_from_slice(&chunk[..n]);
                    }
                    records.lock().unwrap().push((
                        request_headers,
                        serde_json::from_slice(&buffer[header_end..header_end + length]).unwrap(),
                    ));
                    signal.notify_one();
                    if lost && body.is_empty() {
                        return;
                    }
                    tokio::time::sleep(delay).await;
                    let head = if chunked {
                        format!("HTTP/1.1 {status} fixture\r\ncontent-type: application/json\r\ntransfer-encoding: chunked\r\nconnection: close\r\n{headers}\r\n")
                    } else {
                        format!("HTTP/1.1 {status} fixture\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{headers}\r\n", body.len() + usize::from(lost) * 100)
                    };
                    if stream.write_all(head.as_bytes()).await.is_err() {
                        return;
                    }
                    if chunked {
                        for chunk in body.as_bytes().chunks(17) {
                            let wire = format!(
                                "{:x}\r\n{}\r\n",
                                chunk.len(),
                                std::str::from_utf8(chunk).unwrap()
                            );
                            if stream.write_all(wire.as_bytes()).await.is_err() {
                                return;
                            }
                        }
                        let _ = stream.write_all(b"0\r\n\r\n").await;
                    } else {
                        let _ = stream.write_all(body.as_bytes()).await;
                    }
                });
            }
        });
        Self {
            base,
            seen,
            connections,
            received,
            task,
        }
    }
    async fn json(body: Value) -> Self {
        Self::serve(
            200,
            body.to_string(),
            "x-request-id: transport-17\r\n",
            false,
            Duration::ZERO,
            false,
        )
        .await
    }
}

#[derive(Default)]
struct TestAuthority {
    accepted: Mutex<BTreeMap<String, String>>,
    calls: AtomicUsize,
}
#[async_trait]
impl DispatchAuthority for TestAuthority {
    async fn admit(&self, request: &DispatchRequest) -> Result<Admission, AdmissionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut accepted = self.accepted.lock().unwrap();
        match accepted.get(&request.context.attempt_id) {
            Some(digest) if digest == &request.request_digest => Ok(Admission::AlreadyKnown),
            Some(_) => Err(AdmissionError::Denied),
            None => {
                accepted.insert(
                    request.context.attempt_id.clone(),
                    request.request_digest.clone(),
                );
                Ok(Admission::Fresh)
            }
        }
    }
}
struct Refuse(AdmissionError);
#[async_trait]
impl DispatchAuthority for Refuse {
    async fn admit(&self, _: &DispatchRequest) -> Result<Admission, AdmissionError> {
        Err(self.0)
    }
}

fn config(base_url: &str) -> GatewayConfig {
    GatewayConfig {
        base_url: base_url.into(),
        endpoint_policy: EndpointPolicy::LoopbackFixture,
        model: "fixture-model".into(),
        credential_ref: "fixture-key-v1".into(),
        api_key: KEY.into(),
        limits: ModelLimits {
            max_request_bytes: 32_768,
            max_response_bytes: 65_536,
            max_output_tokens: 100,
            timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(1),
        },
    }
}
fn request() -> ModelRequest {
    ModelRequest {
        context: AttemptContext {
            attempt_id: "attempt-1".into(),
            run_id: "run-1".into(),
            owner: "owner-1".into(),
            agent: "agent-1".into(),
            device: "device-1".into(),
            space: "space-1".into(),
            authority_version: "authority-1".into(),
            definition_version: "definition-1".into(),
            price_version: "price-1".into(),
        },
        operation: Operation::ChatCompletion,
        messages: vec![
            InputMessage::System {
                text: "pinned instructions".into(),
            },
            InputMessage::User {
                text: PROMPT.into(),
            },
        ],
        tools: vec![ToolDefinition {
            name: "lookup".into(),
            description: "bounded lookup".into(),
            parameters: json!({"type":"object", "properties":{"q":{"type":"string"}},"required":["q"],"additionalProperties":false}),
        }],
        max_output_tokens: 20,
    }
}
fn reply() -> Value {
    json!({"id":"response-23", "object":"chat.completion", "created":1, "model":"fixture-model",
        "choices":[{"index":0,"message":{"role":"assistant","content":BODY},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":10,"completion_tokens":0,"total_tokens":10}})
}

mod dispatch;
mod privacy;
mod provider;

#[tokio::test]
async fn preflight_matches_actual_dispatch_without_admission_or_http() {
    let fixture = Fixture::json(reply()).await;
    let gateway = ModelGateway::new(config(&fixture.base)).unwrap();
    let request = request();
    let authority = TestAuthority::default();
    let frozen = gateway.preflight(&request).unwrap();
    assert!(frozen.context == request.context);
    assert!(frozen.descriptor.encoded_request_bytes <= frozen.descriptor.max_request_bytes);
    assert_eq!(authority.calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 0);
    gateway.complete(request.clone(), &authority).await.unwrap();
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
    assert_eq!(
        authority
            .accepted
            .lock()
            .unwrap()
            .get(&request.context.attempt_id),
        Some(&frozen.request_digest)
    );
    let repeated = gateway.preflight(&request).unwrap();
    assert_eq!(repeated.request_digest, frozen.request_digest);
    assert_eq!(authority.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}
