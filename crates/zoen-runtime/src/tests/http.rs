use super::*;
use serde_json::{json, Value};
use std::sync::atomic::AtomicUsize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
};

pub(super) struct Provider {
    pub base: String,
    pub sends: Arc<AtomicUsize>,
    pub received: Arc<Notify>,
    pub release: Arc<Notify>,
    pub paused: Arc<AtomicBool>,
    pub lost: Arc<AtomicBool>,
    pub body: Arc<Mutex<Value>>,
    pub requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Provider {
    pub async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let sends = Arc::new(AtomicUsize::new(0));
        let received = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let paused = Arc::new(AtomicBool::new(false));
        let lost = Arc::new(AtomicBool::new(false));
        let body = Arc::new(Mutex::new(reply()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (counter, signal, gate, pause, drop_reply, response, inputs) = (
            sends.clone(),
            received.clone(),
            release.clone(),
            paused.clone(),
            lost.clone(),
            body.clone(),
            requests.clone(),
        );
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let (counter, signal, gate, pause, drop_reply, response, inputs) = (
                    counter.clone(),
                    signal.clone(),
                    gate.clone(),
                    pause.clone(),
                    drop_reply.clone(),
                    response.clone(),
                    inputs.clone(),
                );
                tokio::spawn(async move {
                    let mut wire = Vec::new();
                    let end;
                    loop {
                        let mut buf = [0; 4096];
                        let Ok(n) = stream.read(&mut buf).await else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        wire.extend_from_slice(&buf[..n]);
                        if let Some(i) = wire.windows(4).position(|v| v == b"\r\n\r\n") {
                            end = i + 4;
                            break;
                        }
                        assert!(wire.len() < 65536);
                    }
                    let headers = String::from_utf8(wire[..end].to_vec()).unwrap();
                    assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    assert!(length <= 32768);
                    while wire.len() < end + length {
                        let mut buf = [0; 4096];
                        let Ok(n) = stream.read(&mut buf).await else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        wire.extend_from_slice(&buf[..n]);
                    }
                    let input: Value = serde_json::from_slice(&wire[end..end + length]).unwrap();
                    assert_eq!(input["model"], "fixture-model");
                    inputs.lock().unwrap().push(input);
                    counter.fetch_add(1, Ordering::SeqCst);
                    signal.notify_one();
                    if pause.load(Ordering::SeqCst) {
                        gate.notified().await;
                    }
                    if drop_reply.load(Ordering::SeqCst) {
                        return;
                    }
                    let text = response.lock().unwrap().to_string();
                    let head = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",text.len());
                    if stream.write_all(head.as_bytes()).await.is_ok() {
                        let _ = stream.write_all(text.as_bytes()).await;
                    }
                });
            }
        });
        Self {
            base,
            sends,
            received,
            release,
            paused,
            lost,
            body,
            requests,
            task,
        }
    }
    pub async fn wait(&self) {
        tokio::time::timeout(Duration::from_secs(3), self.received.notified())
            .await
            .unwrap();
    }
}
pub(super) fn reply() -> Value {
    json!({"id":"fixture-response","object":"chat.completion","created":1,"model":"fixture-model",
      "choices":[{"index":0,"message":{"role":"assistant","content":BODY},"finish_reason":"stop"}],
      "usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6}})
}
