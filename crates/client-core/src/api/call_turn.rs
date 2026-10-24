//! Native RTC borrows the same authenticated Client; no extra account/session.
#[cfg(test)]
use super::{Arc, EventBody, Identity};

use super::{invalid, Client, CoreError, Duration, Op, Privacy, Reply, Role, SpaceKind};

#[derive(Clone, PartialEq, uniffi::Record)]
pub struct CallTurnDto {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
    pub expires_at_ms: i64,
}
impl std::fmt::Debug for CallTurnDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallTurnDto")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish_non_exhaustive()
    }
}

fn scope(
    client: &Client,
    identity: &str,
    device: &str,
    space: &str,
    peer: &str,
) -> Result<String, CoreError> {
    let e = client.lock();
    let account = e
        .account()
        .ok_or_else(|| invalid("Call account unavailable"))?;
    if !e.is_unlocked()
        || !account.registered
        || account.identity != identity
        || account.device != device
        || peer == identity
    {
        return Err(CoreError::Forbidden {
            reason: "Call owner or device changed".into(),
        });
    }
    e.assert_call_peer_allowed(peer)?;
    let state = e.space_state(space)?;
    if state.kind != SpaceKind::Direct
        || state.privacy != Privacy::EndToEnd
        || state.members.len() != 2
        || ![identity, peer].iter().all(|who| {
            state
                .members
                .iter()
                .any(|(id, role)| id == who && *role != Role::Reader)
                && e.identities
                    .get(*who)
                    .is_some_and(|p| p.kind == types::IdentityKind::Person)
        })
    {
        return Err(CoreError::Forbidden {
            reason: "Call owner or participants changed".into(),
        });
    }
    Ok(account.relay_url.clone())
}

#[uniffi::export(async_runtime = "tokio")]
impl Client {
    /// Transport allocation only. Native RTC must independently require the
    /// confirmed call/selected device/MLS-authenticated DTLS fingerprint and use
    /// relay-only ICE. There is no public STUN or plaintext fallback here.
    pub async fn call_turn(
        &self,
        expected_identity_id: String,
        expected_device_id: String,
        space_id: String,
        peer_identity_id: String,
    ) -> Result<CallTurnDto, CoreError> {
        let check = || {
            scope(
                self,
                &expected_identity_id,
                &expected_device_id,
                &space_id,
                &peer_identity_id,
            )
        };
        let relay = check()?;
        self.wait_until_settled(5000).await?;
        if check()? != relay {
            return Err(invalid("Call connection changed"));
        }
        let response = tokio::time::timeout(
            Duration::from_secs(8),
            self.request(Op::CallTurn {
                space: space_id.clone(),
                peer: peer_identity_id.clone(),
            }),
        )
        .await;
        if check()? != relay {
            return Err(invalid("Call connection changed"));
        }
        let Reply::CallTurn(credentials) =
            response.map_err(|_| invalid("Call transport unavailable"))??
        else {
            return Err(invalid("Call transport unavailable"));
        };
        if !credentials.valid() || credentials.expires_at_ms <= crate::engine::now_ms() {
            return Err(invalid("Call transport credentials expired or invalid"));
        }
        Ok(CallTurnDto {
            urls: credentials.urls,
            username: credentials.username,
            credential: credentials.credential,
            expires_at_ms: credentials.expires_at_ms,
        })
    }
}

#[cfg(test)]
mod tests;
