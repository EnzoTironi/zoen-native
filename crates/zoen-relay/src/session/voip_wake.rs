//! Explicit wake requests reference a committed envelope. Only the recipient can
//! decrypt it and decide whether it is a live invitation.
use super::*;

pub(super) fn enabled(push: &crate::push::Pusher) -> bool {
    push.voip_available() && std::env::var("ZOEN_ENABLE_VOIP_WAKE").as_deref() == Ok("1")
}

impl Session {
    pub(super) async fn request_voip_wake(
        &self,
        space: &str,
        peer: &str,
        seq: u64,
    ) -> Result<Reply, String> {
        // Enable only with a deployed PushKit client that verifies the invitation.
        if !enabled(&self.st.push) {
            return Err("voip_wake_unavailable".into());
        }
        if !valid_hex64(peer) || peer == self.identity || space.is_empty() || space.len() > 128 {
            return Err("voip_wake_refused".into());
        }
        let sequence = i64::try_from(seq).map_err(|_| "voip_wake_refused")?;
        self.publish_allowed().map_err(limits::slow_down)?;
        tokio::time::timeout(Duration::from_secs(5), async {
            let event = self
                .st
                .log
                .call_wake_event(space, &self.identity, &self.device, peer, seq)
                .await
                .map_err(|_| "voip_wake_unavailable")?
                .ok_or("voip_wake_refused")?;
            let now = crate::key_package_claims::clock_ms(&self.st.pool)
                .await
                .map_err(|_| "voip_wake_unavailable")?;
            let wake = committed_wake(&event, now).ok_or("voip_wake_refused")?;
            let targets = db::voip_push_targets(&self.st.pool, peer, &self.identity, space)
                .await
                .map_err(|_| "voip_wake_unavailable")?;
            if targets.is_empty() || !self.still_authorized().await {
                return Ok(Reply::Done);
            }
            if db::claim_voip_wake(&self.st.pool, space, sequence, wake.expires_at_ms())
                .await
                .map_err(|_| "voip_wake_unavailable")?
            {
                for target in targets {
                    if self.st.push.call_wake(&target, &wake).await == crate::push::Outcome::Gone {
                        if db::drop_push_token(&self.st.pool, &target).await.is_err() {
                            tracing::warn!("failed to remove invalid VoIP registration");
                        }
                    }
                }
            }
            Ok(Reply::Done)
        })
        .await
        .map_err(|_| "voip_wake_unavailable".to_string())?
    }
}

fn committed_wake(event: &protocol::Sequenced, now: i64) -> Option<protocol::push::CallWake> {
    let published = event.env.at_ms();
    if published > now.saturating_add(5_000) {
        return None;
    }
    let expiry = published.checked_add(protocol::push::CALL_WAKE_WINDOW_MS)?;
    let nonce = event.hash.get(..32)?.to_owned();
    protocol::push::CallWake::new(nonce, expiry, now).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_wake_has_a_fixed_deadline_and_rejects_future_events() -> anyhow::Result<()> {
        let author = event_log::Author::device(
            &event_log::Signer::generate()?,
            event_log::Signer::generate()?,
        );
        let published = 50_000;
        let event = protocol::Sequenced {
            seq: 7,
            prev: "00".repeat(32),
            hash: "ab".repeat(32),
            env: protocol::Envelope::sealed(
                &author,
                "direct",
                &types::new_ulid(published)?,
                published,
                None,
                protocol::Sealed::new(protocol::SealedKind::Application, 3, vec![9; 48]),
            )?,
        };
        for now in [45_000, 50_000, 79_999] {
            let wake =
                committed_wake(&event, now).ok_or_else(|| anyhow::anyhow!("wake refused"))?;
            assert_eq!(wake.expires_at_ms(), 80_000);
        }
        assert!(committed_wake(&event, 44_999).is_none());
        assert!(committed_wake(&event, 80_000).is_none());
        Ok(())
    }
}
