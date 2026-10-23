//! Forward the real handshake/request and discard a selected actual response.
use super::*;

pub(super) async fn drop_matching(
    listener: tokio::net::TcpListener,
    url: String,
    matches_frame: fn(&ServerFrame) -> bool,
    dropped: oneshot::Sender<ServerFrame>,
) {
    let (stream, _) = listener.accept().await.unwrap();
    let mut downstream = tokio_tungstenite::accept_async(stream).await.unwrap();
    let (mut upstream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    loop {
        tokio::select! {
            incoming = downstream.next() => {
                let message = incoming.expect("downstream ended before selected response").unwrap();
                upstream.send(message).await.unwrap();
            }
            outgoing = upstream.next() => {
                let message = outgoing.expect("relay ended before selected response").unwrap();
                if let Message::Binary(bytes) = &message {
                    let frame = ServerFrame::decode(bytes).unwrap();
                    if matches_frame(&frame) {
                        dropped.send(frame).unwrap();
                        return;
                    }
                }
                downstream.send(message).await.unwrap();
            }
        }
    }
}
