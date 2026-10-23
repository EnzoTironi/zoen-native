//! Transparent, bounded transport fault: accept a real upstream commit, lose its reply.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
};

pub struct ReplyLoss {
    task: JoinHandle<()>,
    pub dropped: Arc<AtomicUsize>,
}
impl Drop for ReplyLoss {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl ReplyLoss {
    pub async fn start(port: u16, upstream: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
        let dropped = Arc::new(AtomicUsize::new(0));
        let count = dropped.clone();
        let task = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    incoming = listener.accept() => {
                        let (socket, _) = incoming.unwrap();
                        let count = count.clone();
                        tasks.spawn(async move {
                            // A malformed client cannot retain an unbounded fixture connection.
                            let _ = tokio::time::timeout(std::time::Duration::from_secs(20),
                                forward(socket, upstream, count)).await;
                        });
                    }
                    Some(result) = tasks.join_next(), if !tasks.is_empty() => { result.unwrap(); }
                }
            }
        });
        Self { task, dropped }
    }
}
async fn forward(
    mut client: TcpStream,
    port: u16,
    dropped: Arc<AtomicUsize>,
) -> std::io::Result<()> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() == 16 * 1024 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        header.push(client.read_u8().await?);
    }
    let text = std::str::from_utf8(&header).map_err(|_| std::io::ErrorKind::InvalidData)?;
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap();
    let publication = first.starts_with("POST /v1/sites/publications ");
    let mut length = 0;
    let mut forwarded = format!("{first}\r\n");
    for line in lines.filter(|s| !s.is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or(std::io::ErrorKind::InvalidData)?;
        if key.eq_ignore_ascii_case("content-length") {
            length = value
                .trim()
                .parse::<usize>()
                .map_err(|_| std::io::ErrorKind::InvalidData)?;
        }
        if key.eq_ignore_ascii_case("transfer-encoding") {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        if !key.eq_ignore_ascii_case("connection") {
            forwarded.push_str(line);
            forwarded.push_str("\r\n");
        }
    }
    if length > 13 * 1024 * 1024 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    forwarded.push_str("Connection: close\r\n\r\n");
    let mut body = vec![0; length];
    client.read_exact(&mut body).await?;
    let mut server = TcpStream::connect(("127.0.0.1", port)).await?;
    server.write_all(forwarded.as_bytes()).await?;
    server.write_all(&body).await?;
    let mut response = Vec::new();
    // Read at most one extra byte to reject an oversized upstream reply.
    (&mut server)
        .take(512 * 1024 + 1)
        .read_to_end(&mut response)
        .await?;
    assert!(
        response.len() <= 512 * 1024,
        "upstream fixture reply exceeded bound"
    );
    if publication
        && response.starts_with(b"HTTP/1.1 200 ")
        && dropped
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        // No replacement response: the actual committed server reply is discarded.
        client.shutdown().await?;
    } else {
        client.write_all(&response).await?;
    }
    Ok(())
}
