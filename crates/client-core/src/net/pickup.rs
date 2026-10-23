//! Cancelable immutable pairing-mailbox transport under one deadline.

use super::*;

/// Waits for a link box as a device with no account yet (ADR 0045): it signs in as itself
/// (its device key as identity, self-certified), which lets it fetch and nothing else.
pub(crate) async fn fetch_link_box(
    relay_url: &str,
    key: &impl crate::link_pickup::PickupSigning,
    id: &str,
    deadline: tokio::time::Instant,
) -> Result<Vec<u8>, String> {
    let expired = || "O prazo para conectar este aparelho acabou. Gere outro código.".to_string();
    if tokio::time::Instant::now() >= deadline {
        return Err(expired());
    }
    let pickup = async {
        loop {
            key.check()?;
            match fetch_link_attempt(relay_url, key, id).await {
                Ok(bytes) => return Ok(bytes),
                Err(PickupError::Fatal(message)) => return Err(message),
                Err(PickupError::Transport) => {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                }
            }
        }
    };
    // One deadline owns connect, authentication, every receive/send and backoff.
    // Dropping this future also drops the socket and cancels all pickup work.
    let result = tokio::time::timeout_at(deadline, pickup)
        .await
        .map_err(|_| expired())?;
    if tokio::time::Instant::now() >= deadline {
        return Err(expired());
    }
    result
}

pub(super) async fn fetch_link_attempt(
    relay_url: &str,
    key: &impl crate::link_pickup::PickupSigning,
    id: &str,
) -> Result<Vec<u8>, PickupError> {
    key.check().map_err(PickupError::Fatal)?;
    let url = ws_url(relay_url);
    let (ws, _) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async_with_config(url.as_str(), None, true),
    )
    .await
    .map_err(|_| PickupError::Transport)?
    .map_err(|_| PickupError::Transport)?;
    let (mut sink, mut stream) = ws.split();
    let me = key.id().map_err(PickupError::Fatal)?;
    send(
        &mut sink,
        &ClientFrame::Hello {
            protocol: PROTOCOL_VERSION,
            capabilities: CAPABILITIES.iter().map(|c| c.to_string()).collect(),
            identity: me.clone(),
            device: me.clone(),
            cert: key
                .sign(&event_log::device_cert_message(&me))
                .map_err(PickupError::Fatal)?,
        },
    )
    .await?;
    let (nonce, relay) = match recv(&mut stream).await? {
        ServerFrame::Challenge {
            nonce,
            relay,
            protocol,
            ..
        } if protocol >= MIN_PROTOCOL_VERSION => (nonce, relay),
        ServerFrame::Challenge { protocol, .. } => {
            return Err(PickupError::Fatal(format!(
                "O servidor precisa ser atualizado antes de conectar (versão {protocol})."
            )));
        }
        ServerFrame::Error { message, .. } => return Err(PickupError::Fatal(message)),
        _ => {
            return Err(PickupError::Fatal(
                "Resposta inesperada ao conectar.".into(),
            ));
        }
    };
    send(
        &mut sink,
        &ClientFrame::Auth {
            sig: key
                .sign(&auth_message(&nonce, &relay))
                .map_err(PickupError::Fatal)?,
        },
    )
    .await?;
    loop {
        match recv(&mut stream).await? {
            ServerFrame::Ready { .. } => break,
            ServerFrame::Error { message, .. } => return Err(PickupError::Fatal(message)),
            _ => continue,
        }
    }
    let mut n = 0u64;
    loop {
        n += 1;
        key.check().map_err(PickupError::Fatal)?;
        let op = Op::FetchLink { id: id.to_string() };
        send(&mut sink, &ClientFrame::Req { id: n, op }).await?;
        let result = loop {
            match recv(&mut stream).await? {
                ServerFrame::Res { id, result } if id == n => break result,
                ServerFrame::Error { message, .. } => return Err(PickupError::Fatal(message)),
                _ => continue,
            }
        };
        match result {
            Ok(Reply::Link(Some(b))) => return Ok(b),
            Ok(Reply::Link(None)) => {}
            Ok(_) => {
                return Err(PickupError::Fatal(
                    "Resposta inesperada ao conectar.".into(),
                ));
            }
            Err(e) => return Err(PickupError::Fatal(e)),
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
}
