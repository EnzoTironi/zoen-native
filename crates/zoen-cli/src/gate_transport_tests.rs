//! Decision requests must not be forwarded or admitted by an HTTP error body.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refuses_redirects_and_http_errors_without_another_dispatch(
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    for status in [301, 302, 307, 308, 400, 429, 500, 503] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let peer = tokio::spawn(async move {
            let mut requests = 0;
            while let Ok(accepted) =
                tokio::time::timeout(Duration::from_millis(500), listener.accept()).await
            {
                let (mut stream, _) = accepted?;
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 1024];
                    let size = stream.read(&mut buffer).await?;
                    if size == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..size]);
                    if request.windows(4).any(|part| part == b"\r\n\r\n") {
                        break;
                    }
                }
                requests += 1;
                // Even a valid-looking decision body must not admit a failed
                // HTTP request. Same-origin redirects must not replay its state.
                let response = format!("HTTP/1.1 {status} fixture\r\nLocation: http://{address}/forwarded\r\nRetry-After: 0\r\nContent-Length: 16\r\nConnection: close\r\n\r\n{{\"success\":true}}");
                stream.write_all(response.as_bytes()).await?;
                stream.shutdown().await?;
                if requests == 3 {
                    break;
                }
            }
            Ok::<_, std::io::Error>(requests)
        });
        let transport = ReqwestTransport::with_limits(
            reqwest::Client::builder().no_proxy(),
            Duration::from_secs(2),
            Duration::from_secs(1),
            128,
        )?;
        assert_eq!(
            transport.post_json(
                &format!("http://{address}/decision"),
                Some("synthetic"),
                "{}"
            ),
            Err("status".into()),
            "HTTP {status} must fail closed"
        );
        assert_eq!(peer.await??, 1, "HTTP {status} must not replay the POST");
    }
    Ok(())
}
