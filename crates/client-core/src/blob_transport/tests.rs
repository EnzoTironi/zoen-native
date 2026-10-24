use super::*;
use std::io::{Read, Write};

#[tokio::test]
async fn only_actual_exact_signed_put_success_constructs_a_receipt() {
    for status in [Some(201), Some(200), Some(204), Some(400), Some(403), None] {
        let key = Signer::generate().unwrap();
        let device = key.id();
        let bytes = vec![91; 4096];
        let sha = hex::encode(Sha256::digest(&bytes));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (expected, expected_sha) = (bytes.clone(), sha.clone());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut wire = Vec::new();
            while !wire.ends_with(b"\r\n\r\n") {
                let mut one = [0];
                socket.read_exact(&mut one).unwrap();
                wire.extend(one);
                assert!(wire.len() < 16384);
            }
            let head = String::from_utf8(wire).unwrap();
            assert!(head.starts_with(&format!("PUT /v1/blobs/{expected_sha} HTTP/1.1\r\n")));
            let header = |name: &str| {
                head.lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case(name)
                            .then(|| value.trim().to_owned())
                    })
                    .unwrap()
            };
            let length: usize = header("content-length").parse().unwrap();
            assert_eq!(length, expected.len());
            let mut body = vec![0; length];
            socket.read_exact(&mut body).unwrap();
            assert_eq!(body, expected);
            assert_eq!(header("x-zoen-device"), device);
            assert_eq!(header("x-zoen-ts"), "1234");
            assert!(event_log::verify_sig(
                &device,
                &protocol::blob_put_message(&expected_sha, 1234, "real-test-relay"),
                &header("x-zoen-sig")
            ));
            if let Some(status) = status {
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
            }
        });
        let http = crate::https::client()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let result = put(&http, &base, "real-test-relay", &key, &sha, bytes, 1234).await;
        match status {
            Some(200 | 201) => {
                let receipt = result.unwrap();
                assert_eq!(receipt.blob(), sha);
                assert_eq!(receipt.device(), key.id());
                assert_eq!(receipt.bytes(), 4096);
            }
            Some(status) => {
                assert!(matches!(result,Err(BlobUploadError::Refused(actual)) if actual==status))
            }
            None => assert!(matches!(result, Err(BlobUploadError::Unavailable))),
        }
        server.join().unwrap();
    }
}

#[tokio::test]
async fn changed_ciphertext_is_refused_before_any_network_attempt() {
    let http = crate::https::client().build().unwrap();
    assert!(matches!(
        put(
            &http,
            "http://127.0.0.1:1",
            "test",
            &Signer::generate(),
            &"a".repeat(64),
            vec![1, 2, 3],
            1234
        )
        .await,
        Err(BlobUploadError::Invalid)
    ));
}
