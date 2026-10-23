//! Lose genuine upstream ACKs after actual blob storage / sequencer admission.
//! This proxy cannot manufacture successful frames or acknowledge ciphertext.
use super::*;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::AtomicUsize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_tungstenite::tungstenite::Message;

pub(super) struct Proxy {
    pub(super) address: std::net::SocketAddr,
    pub(super) drop_blob: Arc<AtomicBool>,
    pub(super) drop_publish: Arc<AtomicBool>,
    pub(super) blob_dropped: Arc<AtomicUsize>,
    pub(super) publish_dropped: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Proxy {
    pub(super) async fn start(upstream: std::net::SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let drop_blob = Arc::new(AtomicBool::new(true));
        let drop_publish = Arc::new(AtomicBool::new(true));
        let blob_dropped = Arc::new(AtomicUsize::new(0));
        let publish_dropped = Arc::new(AtomicUsize::new(0));
        let (b, p, bc, pc) = (
            drop_blob.clone(),
            drop_publish.clone(),
            blob_dropped.clone(),
            publish_dropped.clone(),
        );
        let task = tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    incoming = listener.accept() => {
                        let (socket,_) = incoming.unwrap();
                        let (b,p,bc,pc) = (b.clone(),p.clone(),bc.clone(),pc.clone());
                        children.spawn(async move {
                            let _ = tokio::time::timeout(Duration::from_secs(45), forward(socket, upstream, b,p,bc,pc)).await;
                        });
                    }
                    Some(result) = children.join_next(), if !children.is_empty() => { result.unwrap(); }
                }
            }
        });
        Self {
            address,
            drop_blob,
            drop_publish,
            blob_dropped,
            publish_dropped,
            task,
        }
    }
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn forward(
    mut client: TcpStream,
    upstream: std::net::SocketAddr,
    drop_blob: Arc<AtomicBool>,
    drop_publish: Arc<AtomicBool>,
    blob_count: Arc<AtomicUsize>,
    publish_count: Arc<AtomicUsize>,
) -> std::io::Result<()> {
    let mut peek = [0; 32];
    loop {
        if client.peek(&mut peek).await? >= 14 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    if peek.starts_with(b"GET /v1/sync ") {
        let front = tokio_tungstenite::accept_async(client).await.unwrap();
        let (back, _) = tokio_tungstenite::connect_async(format!("ws://{upstream}/v1/sync"))
            .await
            .unwrap();
        let (mut fs, mut fr) = front.split();
        let (mut bs, mut br) = back.split();
        loop {
            tokio::select! {
                msg = fr.next() => {
                    let Some(Ok(msg)) = msg else { break };
                    if bs.send(msg).await.is_err() { break }
                }
                msg = br.next() => {
                    let Some(Ok(msg)) = msg else { break };
                    if let Message::Binary(bytes) = &msg {
                        if matches!(protocol::ServerFrame::decode(bytes), Ok(protocol::ServerFrame::Accepted { .. }))
                            && drop_publish.swap(false, Ordering::SeqCst) {
                            publish_count.fetch_add(1,Ordering::SeqCst);
                            break;
                        }
                    }
                    if fs.send(msg).await.is_err() { break }
                }
            }
        }
        return Ok(());
    }
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        assert!(header.len() < 16384);
        header.push(client.read_u8().await?);
    }
    let text = std::str::from_utf8(&header).unwrap();
    let length: usize = text
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap_or(0);
    assert!(length <= 13 * 1024 * 1024);
    let is_put = text.starts_with("PUT /v1/blobs/");
    let mut body = vec![0; length];
    client.read_exact(&mut body).await?;
    let first = text.lines().next().unwrap();
    let mut forwarded = format!("{first}\r\n");
    for line in text.lines().skip(1).filter(|line| !line.is_empty()) {
        if !line.to_ascii_lowercase().starts_with("connection:") {
            forwarded.push_str(line);
            forwarded.push_str("\r\n");
        }
    }
    forwarded.push_str("Connection: close\r\n\r\n");
    let mut server = TcpStream::connect(upstream).await?;
    server.write_all(forwarded.as_bytes()).await?;
    server.write_all(&body).await?;
    let mut response = Vec::new();
    (&mut server).take(65537).read_to_end(&mut response).await?;
    assert!(response.len() <= 65536);
    if is_put
        && (response.starts_with(b"HTTP/1.1 200 ") || response.starts_with(b"HTTP/1.1 201 "))
        && drop_blob.swap(false, Ordering::SeqCst)
    {
        blob_count.fetch_add(1, Ordering::SeqCst);
        client.shutdown().await?;
    } else {
        client.write_all(&response).await?;
    }
    Ok(())
}
