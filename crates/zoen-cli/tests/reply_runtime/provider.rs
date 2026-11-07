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
        Self::with_history(false).await
    }
    pub async fn with_history(history: bool) -> Result<Self, Box<dyn std::error::Error>> {
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
                            let turn = counter.fetch_add(1, Ordering::SeqCst);
                            if history && turn == 1 {
                                let messages = input["messages"]
                                    .as_array()
                                    .ok_or_else(|| std::io::Error::other("missing messages"))?;
                                assert!(messages.iter().all(|m| m["content"].is_string()));
                                let tool = messages
                                    .last()
                                    .ok_or_else(|| std::io::Error::other("missing result"))?;
                                assert_eq!(tool["role"], "tool");
                                assert_eq!(tool["tool_call_id"], "history-call");
                                let page: serde_json::Value = serde_json::from_str(
                                    tool["content"]
                                        .as_str()
                                        .ok_or_else(|| std::io::Error::other("tool content"))?,
                                )?;
                                assert_eq!(page["hits"].as_array().map(Vec::len), Some(1));
                            }
                            let mut body = serde_json::json!({"id":"local-reply","object":"chat.completion","created":1,"model":"reply-fixture","choices":[{"index":0,"message":{"role":"assistant","content":ANSWER},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":3,"total_tokens":6}});
                            if history && turn == 0 {
                                body["choices"][0]["message"] = serde_json::json!({"role":"assistant","content":null,"tool_calls":[{"id":"history-call","type":"function","function":{"name":"search_conversation_history","arguments":"{\"query\":\"conversa\",\"limit\":1}"}}]});
                                body["choices"][0]["finish_reason"] = "tool_calls".into();
                            }
                            let body = body.to_string();
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
