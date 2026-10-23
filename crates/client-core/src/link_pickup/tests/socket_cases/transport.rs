//! Shared synthetic pickup peer. Verifies the real Hello/Auth and mailbox request.
use super::*;
use futures_util::{SinkExt, StreamExt};
use protocol::{auth_message, ClientFrame, Op, ServerFrame, CAPABILITIES, PROTOCOL_VERSION};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

pub(super) async fn accept_fetch(
    listener: &tokio::net::TcpListener,
    box_id: &str,
) -> (WebSocketStream<tokio::net::TcpStream>, u64) {
    let (stream, _) = tokio::time::timeout(BOUND, listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
    let Message::Binary(hello) = socket.next().await.unwrap().unwrap() else {
        panic!("binary Hello")
    };
    let ClientFrame::Hello {
        identity,
        device,
        cert,
        protocol,
        capabilities,
    } = ClientFrame::decode(&hello).unwrap()
    else {
        panic!("Hello")
    };
    assert_eq!(identity, device);
    assert_eq!(protocol, PROTOCOL_VERSION);
    assert_eq!(
        capabilities,
        CAPABILITIES
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    );
    assert!(event_log::verify_sig(
        &device,
        &event_log::device_cert_message(&device),
        &cert
    ));
    let nonce = "owned-pickup-regression-challenge";
    let relay = "owned-pickup-regression-relay";
    socket
        .send(Message::Binary(
            ServerFrame::Challenge {
                nonce: nonce.into(),
                relay: relay.into(),
                protocol: PROTOCOL_VERSION,
                capabilities: Vec::new(),
                server_time_ms: None,
            }
            .encode()
            .into(),
        ))
        .await
        .unwrap();
    let Message::Binary(auth) = socket.next().await.unwrap().unwrap() else {
        panic!("binary Auth")
    };
    let ClientFrame::Auth { sig } = ClientFrame::decode(&auth).unwrap() else {
        panic!("Auth")
    };
    assert!(event_log::verify_sig(
        &device,
        &auth_message(nonce, relay),
        &sig
    ));
    socket
        .send(Message::Binary(
            ServerFrame::Ready {
                identity,
                registered: false,
            }
            .encode()
            .into(),
        ))
        .await
        .unwrap();
    let Message::Binary(request) = socket.next().await.unwrap().unwrap() else {
        panic!("binary FetchLink")
    };
    let ClientFrame::Req {
        id: request_id,
        op: Op::FetchLink { id },
    } = ClientFrame::decode(&request).unwrap()
    else {
        panic!("FetchLink")
    };
    assert_eq!(request_id, 1);
    assert_eq!(id, box_id);
    (socket, request_id)
}
