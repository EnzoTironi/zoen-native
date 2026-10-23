use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn sites_unfinished_body_expires_and_releases_its_handler_slot() {
    let gate = HttpGate {
        deadline: Duration::from_millis(20),
        ..HttpGate::new()
    };
    let router = Router::new()
        .route("/", post(|bytes: axum::body::Bytes| async { bytes }))
        .layer(axum::middleware::from_fn_with_state(
            gate.clone(),
            admission,
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream
        .write_all(b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\n")
        .await
        .unwrap();
    let mut bytes = [0; 512];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert!(std::str::from_utf8(&bytes[..read])
        .unwrap()
        .starts_with("HTTP/1.1 408"));
    assert_eq!(gate.slots.available_permits(), 16);
    let response = reqwest::Client::new()
        .post(format!("http://{address}/"))
        .body("ready")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"ready");
    server.abort();
}
