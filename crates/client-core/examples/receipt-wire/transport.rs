//! Byte-preserving HTTP/WebSocket forwarding for the owned receipt journey.
use super::Capture;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message};

pub(super) struct Proxy {
    pub(super) url: String,
    pub(super) stop: watch::Sender<bool>,
    pub(super) task: tokio::task::JoinHandle<()>,
    pub(super) generation: Arc<AtomicU64>,
    pub(super) active: Arc<AtomicU64>,
}

// Peeking preserves every request byte, including any body already coalesced
// with the headers. Only the WebSocket handshake is terminated by this proxy;
// normal HTTP remains an opaque TCP tunnel to the same owned relay endpoint.
async fn request_kind(stream: &TcpStream) -> std::io::Result<(String, bool)> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut header = [0u8; 16 * 1024];
        loop {
            let count = stream.peek(&mut header).await?;
            if count == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
            }
            if let Some(end) = header[..count].windows(4).position(|b| b == b"\r\n\r\n") {
                let text = std::str::from_utf8(&header[..end])
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                let mut lines = text.split("\r\n");
                let request = lines.next().unwrap_or_default().to_string();
                let websocket = lines.any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("upgrade")
                            && value.trim().eq_ignore_ascii_case("websocket")
                    })
                });
                return Ok((request, websocket));
            }
            if count == header.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Owned proxy HTTP header capacity exceeded",
                ));
            }
            // A partial header remains readable, so avoid spinning on peek.
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
}

async fn tunnel_http(
    mut client: TcpStream,
    upstream: std::net::SocketAddr,
) -> std::io::Result<(u64, u64)> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut server = TcpStream::connect(upstream).await?;
        copy_bidirectional(&mut client, &mut server).await
    })
    .await
    .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
}

struct ActiveConnection(Arc<AtomicU64>);
impl ActiveConnection {
    fn new(count: Arc<AtomicU64>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count)
    }
}
impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(super) async fn proxy(label: &str, target: &str, capture: Arc<Capture>) -> Proxy {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let label = label.to_string();
    let endpoint = reqwest::Url::parse(target).unwrap();
    let upstream = std::net::SocketAddr::from(([127, 0, 0, 1], endpoint.port().unwrap()));
    let target = target.to_string();
    let generation = Arc::new(AtomicU64::new(0));
    let active = Arc::new(AtomicU64::new(0));
    let (stop, mut ended) = watch::channel(false);
    let generations = generation.clone();
    let count = active.clone();
    let task = tokio::spawn(async move {
        let mut jobs = JoinSet::new();
        loop {
            tokio::select! {
             _=ended.changed()=>break,
             Some(result)=jobs.join_next(),if !jobs.is_empty()=>{result.unwrap();},
             accepted=listener.accept()=>{
              assert!(jobs.len()<8,"Owned proxy connection capacity exceeded");
              let(stream,_)=accepted.unwrap();let target=target.clone();let label=label.clone();let capture=capture.clone();
              let generations=generations.clone();let count=count.clone();
              jobs.spawn(async move{
               let (request, websocket)=match request_kind(&stream).await {
                Ok(kind)=>kind,
                Err(error)=>{capture.transport(json!({"client":label,"transport_error":error.to_string()}));return;}
               };
               capture.transport(json!({"client":label,"http_request":request,"websocket":websocket}));
               if !websocket {
                let result=tunnel_http(stream,upstream).await;
                capture.transport(match result {
                 Ok((sent,received))=>json!({"client":label,"http_tunnel_complete":true,"sent_bytes":sent,"received_bytes":received}),
                 Err(error)=>json!({"client":label,"http_tunnel_error":error.to_string()}),
                });
                return;
               }
               // Never conceal an incorrectly composed client URL by forwarding
               // its handshake to the canonical upstream path anyway.
               if request != "GET /v1/sync HTTP/1.1" {
                capture.transport(json!({"client":label,"websocket_path_refused":request}));
                return;
               }
               let generation=generations.fetch_add(1,Ordering::SeqCst)+1;
               let _active=ActiveConnection::new(count);
               let mut client=match accept_async(stream).await {
                Ok(client)=>client,
                Err(error)=>{capture.transport(json!({"client":label,"websocket_accept_error":error.to_string()}));return;}
               };
               let mut server=match connect_async(target.as_str()).await {
                Ok((server,_))=>server,
                Err(error)=>{capture.transport(json!({"client":label,"websocket_connect_error":error.to_string()}));return;}
               };
               loop{tokio::select!{
                incoming=client.next()=>match incoming{Some(Ok(message))=>{
                 if let Message::Binary(bytes)=&message{capture.record(&label,generation,true,bytes);}
                 let close=message.is_close();if server.send(message).await.is_err()||close{break}
                },_=>break},
                incoming=server.next()=>match incoming{Some(Ok(message))=>{
                 if let Message::Binary(bytes)=&message{capture.record(&label,generation,false,bytes);}
                 let close=message.is_close();if client.send(message).await.is_err()||close{break}
                },_=>break},
               }}
              });
             }
            }
        }
        jobs.abort_all();
        while let Some(result) = jobs.join_next().await {
            assert!(result.is_ok() || result.unwrap_err().is_cancelled());
        }
    });
    Proxy {
        // Public Client accepts the relay BASE and appends /v1/sync or /v1/blobs.
        // The CLI target remains the exact upstream WebSocket endpoint.
        url: format!("ws://{address}"),
        stop,
        task,
        generation,
        active,
    }
}

#[cfg(test)]
mod transport_tests {
    use super::{proxy, request_kind, tunnel_http, Capture};
    use crate::Vault;
    use client_core::Client;
    use futures_util::StreamExt;
    use protocol::ClientFrame;
    use std::{
        sync::{atomic::AtomicU64, Arc, Mutex},
        time::Duration,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    // These use real loopback TCP, but prove only the transparent QA transport,
    // not relay authentication, actual recovery upload, or receipt delivery.
    #[tokio::test]
    async fn fragmented_http_classification_preserves_header_and_body() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let request =
            b"PUT /v1/blobs/opaque HTTP/1.1\r\nHost: owned\r\nContent-Length: 4\r\n\r\n\x00\xffab";
        let expected = request.to_vec();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let kind = request_kind(&stream).await.unwrap();
            assert_eq!(kind, ("PUT /v1/blobs/opaque HTTP/1.1".into(), false));
            let mut actual = vec![0; expected.len()];
            stream.read_exact(&mut actual).await.unwrap();
            assert_eq!(actual, expected);
        });
        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(&request[..22]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(3)).await;
        client.write_all(&request[22..]).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn genuine_websocket_header_is_detected_case_insensitively() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            request_kind(&stream).await.unwrap()
        });
        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                b"GET /v1/sync HTTP/1.1\r\nuPgRaDe: WebSocket\r\nConnection: Upgrade\r\n\r\n",
            )
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap(),
            ("GET /v1/sync HTTP/1.1".into(), true)
        );
    }

    #[tokio::test]
    async fn http_request_and_refusal_response_are_byte_exact() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_address = upstream.local_addr().unwrap();
        let request = b"PUT /v1/blobs/opaque HTTP/1.1\r\nHost: untouched\r\nContent-Length: 4\r\n\r\n\x00\xffab";
        let response =
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 3\r\nConnection: close\r\n\r\nno!";
        let server = tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await.unwrap();
            let mut actual = vec![0; request.len()];
            stream.read_exact(&mut actual).await.unwrap();
            assert_eq!(actual, request);
            stream.write_all(response).await.unwrap();
        });
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy.local_addr().unwrap();
        let forwarded = tokio::spawn(async move {
            let (stream, _) = proxy.accept().await.unwrap();
            assert!(!request_kind(&stream).await.unwrap().1);
            tunnel_http(stream, upstream_address).await.unwrap()
        });
        let mut client = TcpStream::connect(proxy_address).await.unwrap();
        client.write_all(request).await.unwrap();
        client.shutdown().await.unwrap();
        let mut actual = vec![];
        tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut actual))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(actual, response);
        let (sent, received) = forwarded.await.unwrap();
        assert_eq!(
            (sent, received),
            (request.len() as u64, response.len() as u64)
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn public_client_composes_one_sync_path_from_proxy_base() {
        let directory = std::env::temp_dir().join(types::new_id("receipt-proxy-test").unwrap());
        std::fs::create_dir(&directory).unwrap();
        let capture = Arc::new(Capture {
            file: Mutex::new(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join("transport.jsonl"))
                    .unwrap(),
            ),
            frames: Mutex::new(vec![]),
            bytes: AtomicU64::new(0),
        });
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}/v1/sync", upstream.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = upstream.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let Message::Binary(bytes) = socket.next().await.unwrap().unwrap() else {
                panic!("Actual public Client must send its real Hello");
            };
            assert!(matches!(
                ClientFrame::decode(&bytes),
                Ok(ClientFrame::Hello { .. })
            ));
            // No Challenge/Auth/receipt is fabricated: this unit stops at Hello.
        });
        let proxy = proxy("route-test", &endpoint, capture).await;
        let client = Client::open(
            directory.join("client.sqlite").to_str().unwrap().into(),
            "en-US".into(),
        )
        .unwrap();
        client
            .create_account(
                "Route test".into(),
                "receiptproxytest".into(),
                proxy.url.clone(),
                Arc::new(Vault::default()),
            )
            .unwrap();
        client.start_sync(None).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), server).await;
        client.stop_sync();
        proxy.stop.send(true).unwrap();
        proxy.task.await.unwrap();
        drop(client);
        result.unwrap().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
