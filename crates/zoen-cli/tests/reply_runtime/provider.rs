//! Real local HTTP provider with finite usage and a counted original inference.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
pub const ANSWER: &str = "A conversa continua mesmo com os clientes fechados.";
pub struct Provider {
    pub endpoint: String,
    pub calls: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort()
    }
}
impl Provider {
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}/v1/chat/completions", listener.local_addr()?);
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let counter = counter.clone();
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let mut chunk = [0; 4096];
                    loop {
                        let n = stream.read(&mut chunk).await?;
                        if n == 0 || bytes.len() + n > 32768 {
                            return std::io::Result::Ok(());
                        }
                        bytes.extend_from_slice(&chunk[..n]);
                        if let Some(index) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            let headers = String::from_utf8_lossy(&bytes[..index]);
                            let length = headers
                                .lines()
                                .find_map(|line| {
                                    line.split_once(':')
                                        .filter(|(key, _)| {
                                            key.eq_ignore_ascii_case("content-length")
                                        })
                                        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            if bytes.len() < index + 4 + length {
                                continue;
                            }
                            assert!(headers.starts_with("POST /v1/chat/completions "));
                            let input: serde_json::Value =
                                serde_json::from_slice(&bytes[index + 4..index + 4 + length])?;
                            assert_eq!(input["model"], "reply-fixture");
                            assert!(input["messages"].as_array().is_some_and(|messages| messages
                                .iter()
                                .any(|m| m["role"] == "system")));
                            assert!(input.to_string().contains("Answer the owner"));
                            assert!(input
                                .to_string()
                                .contains("Can you answer with the client closed?"));
                            counter.fetch_add(1, Ordering::SeqCst);
                            let body=serde_json::json!({"id":"local-reply","object":"chat.completion","created":1,"model":"reply-fixture","choices":[{"index":0,"message":{"role":"assistant","content":ANSWER},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":3,"total_tokens":6}}).to_string();
                            let response=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                            stream.write_all(response.as_bytes()).await?;
                            return Ok(());
                        }
                    }
                });
            }
        });
        Ok(Self {
            endpoint,
            calls,
            task,
        })
    }
}
