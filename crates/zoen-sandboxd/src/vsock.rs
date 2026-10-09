//! The host side of the guest control channel: Firecracker's vsock-over-Unix-socket handshake
//! (`CONNECT <port>` / `OK <n>`), then one JSON request and one JSON response.

use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use zoen_guestd::{Request, Response, CONTROL_PORT};

pub async fn request(uds: &Path, req: &Request, limit: Duration) -> std::io::Result<Response> {
    let fut = async {
        let s = UnixStream::connect(uds).await?;
        let (r, mut w) = s.into_split();
        let mut r = BufReader::new(r);
        w.write_all(format!("CONNECT {CONTROL_PORT}\n").as_bytes())
            .await?;
        let mut line = String::new();
        r.read_line(&mut line).await?;
        if !line.starts_with("OK ") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!("vsock handshake: {line:?}"),
            ));
        }
        let mut out = serde_json::to_vec(req)?;
        out.push(b'\n');
        w.write_all(&out).await?;
        line.clear();
        r.read_line(&mut line).await?;
        serde_json::from_str::<Response>(&line).map_err(std::io::Error::other)
    };
    tokio::time::timeout(limit, fut)
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "guest did not answer"))?
}
