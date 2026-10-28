use super::*;

/// A device that proved its keys.
pub(super) struct LoggedIn {
    pub(super) identity: String,
    pub(super) device: String,
    pub(super) cert: String,
    pub(super) registered: bool,
    pub(super) claim_receipts: bool,
    pub(super) complete_reads: bool,
    pub(super) saved_messages: bool,
}

/// Hello → Challenge → Auth. `None` when the client is refused; the reason has been sent.
#[tracing::instrument(
    name = "handshake",
    skip_all,
    fields(outcome = Empty, identity = Empty, registered = Empty)
)]
pub(super) async fn handshake(
    st: &Shared,
    tx: &Outbox,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
    ip: &str,
) -> Option<LoggedIn> {
    let refuse = |outcome: &'static str, frame: ServerFrame| async move {
        tracing::Span::current().record("outcome", outcome);
        let _ = tx.send(frame).await;
        None
    };
    if let Err(wait) = st.limits.connect_ip.check(ip) {
        return refuse(
            "rate_limited",
            ServerFrame::error(ErrorCode::RateLimited, limits::slow_down(wait)),
        )
        .await;
    }
    let hello = tokio::time::timeout(Duration::from_secs(10), recv_frame(stream))
        .await
        .ok()
        .flatten();
    let Some(ClientFrame::Hello {
        protocol,
        capabilities,
        identity,
        device,
        cert,
    }) = hello
    else {
        return refuse(
            "no_hello",
            ServerFrame::error(ErrorCode::Other, "expected hello"),
        )
        .await;
    };
    if protocol < MIN_PROTOCOL_VERSION {
        Metrics::inc(&st.metrics.upgrade_required);
        return refuse(
            "upgrade_required",
            ServerFrame::error(
                ErrorCode::UpgradeRequired,
                format!("protocol {protocol} is too old, this relay needs {MIN_PROTOCOL_VERSION} or newer"),
            ),
        )
        .await;
    }
    if !relay_admission::directory::certificate_valid(&identity, &device, &cert) {
        return refuse(
            "bad_certificate",
            ServerFrame::error(ErrorCode::Unauthorized, "device certificate invalid"),
        )
        .await;
    }
    let capabilities = negotiate(&capabilities);
    let complete_reads = capabilities
        .iter()
        .any(|c| c == protocol::complete_reads::CAPABILITY);
    let saved_messages = capabilities
        .iter()
        .any(|c| c == protocol::saved_messages::CAPABILITY);
    let claim_receipts = capabilities
        .iter()
        .any(|c| c == protocol::KEY_PACKAGE_CLAIM_CAPABILITY);
    let server_time_ms = if claim_receipts {
        match tokio::time::timeout(
            Duration::from_secs(5),
            crate::key_package_claims::clock_ms(&st.pool),
        )
        .await
        {
            Ok(Ok(time)) => Some(time),
            _ => {
                return refuse(
                    "database_unavailable",
                    ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
                )
                .await;
            }
        }
    } else {
        None
    };
    let n = match challenge_nonce(getrandom::getrandom) {
        Ok(nonce) => nonce,
        Err(()) => {
            return refuse(
                "entropy_unavailable",
                ServerFrame::error(ErrorCode::Unavailable, "login challenge unavailable"),
            )
            .await;
        }
    };
    let _ = tx
        .send(ServerFrame::Challenge {
            nonce: n.clone(),
            relay: st.relay_name.clone(),
            protocol: protocol.min(PROTOCOL_VERSION),
            capabilities,
            server_time_ms,
        })
        .await;
    let auth = tokio::time::timeout(Duration::from_secs(10), recv_frame(stream))
        .await
        .ok()
        .flatten();
    let Some(ClientFrame::Auth { sig }) = auth else {
        return refuse(
            "no_auth",
            ServerFrame::error(ErrorCode::Other, "expected auth"),
        )
        .await;
    };
    if !relay_admission::directory::auth_valid(&device, &n, &st.relay_name, &sig) {
        return refuse(
            "bad_signature",
            ServerFrame::error(ErrorCode::Unauthorized, "login signature invalid"),
        )
        .await;
    }
    match db::device_revoked(&st.pool, &device).await {
        Ok(false) => {}
        Ok(true) => {
            return refuse(
                "unlinked",
                ServerFrame::error(ErrorCode::Unauthorized, "this device was unlinked"),
            )
            .await;
        }
        Err(_) => {
            return refuse(
                "unavailable",
                ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
            )
            .await;
        }
    }
    let Ok(registered) = db::is_registered(&st.pool, &identity).await else {
        return refuse(
            "unavailable",
            ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
        )
        .await;
    };
    if registered {
        match db::device_linked(&st.pool, &identity, &device).await {
            Ok(true) => {}
            Ok(false) => {
                return refuse(
                    "not_enrolled",
                    ServerFrame::error(ErrorCode::Unauthorized, "link this device first"),
                )
                .await;
            }
            Err(_) => {
                return refuse(
                    "unavailable",
                    ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
                )
                .await;
            }
        }
    }
    let span = tracing::Span::current();
    span.record("outcome", "ok");
    span.record("identity", tracing::field::display(pseudo(&identity)));
    span.record("registered", registered);
    Some(LoggedIn {
        identity,
        device,
        cert,
        registered,
        claim_receipts,
        complete_reads,
        saved_messages,
    })
}

// Never return partially filled or fallback challenge bytes after an OS failure.
fn challenge_nonce(
    entropy: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<String, ()> {
    let mut bytes = [0u8; 32];
    entropy(&mut bytes).map_err(|_| ())?;
    Ok(hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::challenge_nonce;

    #[test]
    fn failed_or_partial_entropy_cannot_issue_a_login_challenge() {
        assert!(challenge_nonce(|_| Err(getrandom::Error::UNSUPPORTED)).is_err());
        assert!(challenge_nonce(|bytes| {
            bytes[..16].fill(0x7f);
            Err(getrandom::Error::UNSUPPORTED)
        })
        .is_err());
    }

    #[test]
    fn successful_challenge_preserves_all_32_random_bytes() {
        let expected = std::array::from_fn::<_, 32, _>(|index| index as u8);
        let nonce = challenge_nonce(|bytes| {
            bytes.copy_from_slice(&expected);
            Ok(())
        })
        .expect("fixture entropy supplied");
        assert_eq!(nonce.len(), 64);
        assert_eq!(hex::decode(nonce).expect("hex challenge"), expected);
    }
}
