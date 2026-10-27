//! Actual reqwest/HTTP streaming boundaries; no external relay or retained account.
use super::super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn media_body_limit_rejects_headers_and_incremental_overflow_then_accepts_next_response(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let expected = b"synthetic bytes!";
    let limit = expected.len();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), async move {
            let mut observed = Vec::new();
            for path in ["/declared-overflow", "/chunked-overflow", "/valid"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut headers = Vec::new();
                let mut byte = [0];
                while !headers.ends_with(b"\r\n\r\n") {
                    assert!(headers.len() < 8192);
                    assert_eq!(socket.read(&mut byte).await.unwrap(), 1);
                    headers.push(byte[0]);
                }
                let request = String::from_utf8(headers).unwrap();
                assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
                observed.push(path);
                if path == "/declared-overflow" {
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", limit + 1).as_bytes()).await.unwrap();
                    socket.flush().await.unwrap();
                    // No body is provided: rejecting only after buffering it would stall.
                    assert_eq!(socket.read(&mut byte).await.unwrap(), 0);
                } else if path == "/chunked-overflow" {
                    socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
                    socket.write_all(format!("{limit:x}\r\n").as_bytes()).await.unwrap();
                    socket.write_all(expected).await.unwrap();
                    socket.write_all(b"\r\n1\r\nx\r\n").await.unwrap();
                    socket.flush().await.unwrap();
                    // No terminating chunk: overflow must stop before the complete body.
                    assert_eq!(socket.read(&mut byte).await.unwrap(), 0);
                } else {
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {limit}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
                    socket.write_all(expected).await.unwrap();
                }
            }
            observed
        }).await.expect("owned HTTP fixture must complete")
    });
    let http = crate::https::client()?
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert_eq!(
        bounded_blob(&http, &format!("{base}/declared-overflow"), limit).await,
        None
    );
    assert_eq!(
        bounded_blob(&http, &format!("{base}/chunked-overflow"), limit).await,
        None
    );
    assert_eq!(
        bounded_blob(&http, &format!("{base}/valid"), limit).await,
        Some(expected.to_vec())
    );
    assert_eq!(
        server.await.unwrap(),
        ["/declared-overflow", "/chunked-overflow", "/valid"]
    );

    Ok(())
}
