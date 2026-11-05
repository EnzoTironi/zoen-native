//! Consent belongs to the authenticated recipient. The log owns conversation membership.
use super::*;

impl Session {
    pub(super) async fn get_voip_consent(&self, space: &str, peer: &str) -> Result<Reply, String> {
        if !valid_hex64(peer) || peer == self.identity || space.is_empty() || space.len() > 128 {
            return Err("voip_consent_refused".into());
        }
        tokio::time::timeout(
            Duration::from_secs(5),
            db::get_voip_consent(&self.st.pool, &self.identity, peer, space),
        )
        .await
        .map_err(|_| "voip_consent_unavailable")?
        .map(Reply::VoipConsent)
        .map_err(|_| "voip_consent_unavailable".into())
    }

    pub(super) async fn set_voip_consent(
        &self,
        space: &str,
        peer: &str,
        allowed: bool,
    ) -> Result<Reply, String> {
        if !valid_hex64(peer) || peer == self.identity || space.is_empty() || space.len() > 128 {
            return Err("voip_consent_refused".into());
        }
        self.publish_allowed().map_err(limits::slow_down)?;
        tokio::time::timeout(Duration::from_secs(5), async {
            // Revocation must remain available after either party leaves.
            if allowed {
                if !self
                    .st
                    .log
                    .call_pair(space, &self.identity, peer)
                    .await
                    .map_err(|_| "voip_consent_unavailable")?
                {
                    return Err("voip_consent_refused".into());
                }
                let profiles = db::profiles(&self.st.pool, &[self.identity.clone(), peer.into()])
                    .await
                    .map_err(|_| "voip_consent_unavailable")?;
                if profiles.len() != 2 || profiles.iter().any(|p| p.kind != IdentityKind::Person) {
                    return Err("voip_consent_refused".into());
                }
            }
            db::set_voip_consent(
                &self.st.pool,
                &self.identity,
                &self.device,
                peer,
                space,
                allowed,
            )
            .await
            .map_err(|_| "voip_consent_unavailable")?;
            Ok(Reply::Done)
        })
        .await
        .map_err(|_| "voip_consent_unavailable".to_string())?
    }
}
