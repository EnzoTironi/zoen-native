//! Just enough HTTP/1.1 over a Unix socket to drive the Firecracker API.

use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("firecracker API: {0}")]
    Io(#[from] std::io::Error),
    #[error("firecracker API {method} {path}: {status} {body}")]
    Status {
        method: &'static str,
        path: String,
        status: u16,
        body: String,
    },
    #[error("firecracker API {0}: timed out")]
    Timeout(String),
}

/// One request, one connection. Firecracker answers 204 (or 200) on success.
pub async fn call(
    sock: &Path,
    method: &'static str,
    path: &str,
    body: &serde_json::Value,
) -> Result<(), ApiError> {
    let fut = async {
        let mut s = UnixStream::connect(sock).await?;
        let body = body.to_string();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(req.as_bytes()).await?;
        let mut resp = Vec::new();
        // Firecracker may keep the connection open; read until the head and body are in.
        let mut buf = [0u8; 4096];
        loop {
            let n = s.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            resp.extend_from_slice(&buf[..n]);
            if let Some(end) = resp.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&resp[..end]).to_ascii_lowercase();
                let len = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if resp.len() >= end + 4 + len {
                    break;
                }
            }
        }
        let text = String::from_utf8_lossy(&resp).to_string();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse::<u16>().ok())
            .unwrap_or(0);
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(ApiError::Status {
                method,
                path: path.to_string(),
                status,
                body: text.split("\r\n\r\n").nth(1).unwrap_or("").to_string(),
            })
        }
    };
    match tokio::time::timeout(Duration::from_secs(30), fut).await {
        Ok(r) => r,
        Err(_) => Err(ApiError::Timeout(format!("{method} {path}"))),
    }
}
