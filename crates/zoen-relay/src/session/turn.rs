//! Device-authenticated TURN admission. It cannot authorize or decrypt a call.
use super::*;

impl Session {
    pub(super) async fn call_turn(&self, space: &str, peer: &str) -> Result<Reply, String> {
        self.st.turn.check_quota(&self.identity, &self.device)?;
        if !valid_hex64(peer) || peer == self.identity || space.is_empty() || space.len() > 128 {
            return Err("call_transport_refused".into());
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            if !self
                .st
                .log
                .call_pair(space, &self.identity, peer)
                .await
                .map_err(|_| "call_transport_unavailable")?
            {
                return Err("call_transport_refused".into());
            }
            let profiles = db::profiles(&self.st.pool, &[self.identity.clone(), peer.into()])
                .await
                .map_err(|_| "call_transport_unavailable")?;
            if profiles.len() != 2 || profiles.iter().any(|p| p.kind != IdentityKind::Person) {
                return Err("call_transport_refused".into());
            }
            let now = crate::key_package_claims::clock_ms(&self.st.pool)
                .await
                .map_err(|_| "call_transport_unavailable")?;
            // Delivery's existing device-row fence also runs before sending this
            // response. Membership is admitted at the FDB transaction above.
            self.st
                .turn
                .issue(&self.identity, &self.device, now)
                .map(Reply::CallTurn)
        })
        .await
        .map_err(|_| "call_transport_unavailable".to_string())?
    }
}
