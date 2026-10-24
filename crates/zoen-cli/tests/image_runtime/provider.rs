//! Unpaid numeric-loopback HTTP fixture; the actual gateway performs the send.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(super) const PNG: &[u8] = include_bytes!("../../../../apps/zoen/assets/images/icon.png");
pub(super) const PROMPT: &str = "A quiet blue sky";
pub(crate) struct Provider {
    pub endpoint: String,
    pub sends: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Provider {
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!(
            "http://{}/v1/images/generations",
            listener.local_addr().unwrap()
        );
        let sends = Arc::new(AtomicUsize::new(0));
        let counter = sends.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let header = loop {
                    let mut chunk = [0; 4096];
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0, "provider connection ended before headers");
                    bytes.extend_from_slice(&chunk[..n]);
                    assert!(bytes.len() < 32768, "provider request exceeded bound");
                    if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8(bytes[..header].to_vec())
                    .unwrap()
                    .to_lowercase();
                assert!(headers.starts_with("post /v1/images/generations http/1.1\r\n"));
                assert!(headers.contains("authorization: bearer unpaid-local-image-fixture\r\n"));
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                assert!(length < 16384);
                while bytes.len() < header + length {
                    let mut chunk = [0; 4096];
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                }
                let request: Value =
                    serde_json::from_slice(&bytes[header..header + length]).unwrap();
                assert_eq!(request["prompt"], PROMPT);
                assert_eq!(request["model"], "gpt-image-fixture");
                assert_eq!(request["size"], "1024x1024");
                assert_eq!(request["n"], 1);
                counter.fetch_add(1, Ordering::SeqCst);
                let body = serde_json::to_vec(&json!({"created":1,"data":[{"b64_json":STANDARD.encode(PNG)}],"usage":{"input_tokens":5,"output_tokens":2,"total_tokens":7,"input_tokens_details":{"text_tokens":2,"image_tokens":3},"output_tokens_details":{"text_tokens":1,"image_tokens":1}}})).unwrap();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
                socket.write_all(&body).await.unwrap();
            }
        });
        Self {
            endpoint,
            sends,
            task,
        }
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}
