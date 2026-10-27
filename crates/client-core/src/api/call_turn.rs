//! Native RTC borrows the same authenticated Client; no extra account/session.
#[cfg(test)]
use super::{Arc, EventBody, Identity};

use super::{invalid, net_err, Client, CoreError, Duration, Op, Privacy, Reply, Role, SpaceKind};
use crate::net::RequestHandle;

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
    e.native_surface_owner(identity, device)?;
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

struct Allocation<'a> {
    client: &'a Client,
    identity: &'a str,
    device: &'a str,
    space: &'a str,
    peer: &'a str,
    relay: String,
}
impl Allocation<'_> {
    fn channel(
        &self,
        expected: Option<&RequestHandle>,
    ) -> Result<Option<RequestHandle>, CoreError> {
        // Match start_sync's Net -> Engine order, keeping owner/pair validation
        // and the authenticated channel capture in one synchronous section.
        let guard = self
            .client
            .net
            .lock()
            .map_err(|_| invalid("Call connection unavailable"))?;
        if scope(
            self.client,
            self.identity,
            self.device,
            self.space,
            self.peer,
        )? != self.relay
        {
            return Err(invalid("Call connection changed"));
        }
        let channel = guard.as_ref().and_then(|net| {
            net.authenticated_synced_relay_name(&self.relay, self.identity, self.device)
                .is_some()
                .then(|| net.request_handle())
        });
        if expected.is_some_and(|old| {
            channel
                .as_ref()
                .is_none_or(|current| !old.same_channel(current))
        }) {
            return Err(invalid("Call connection changed"));
        }
        Ok(channel)
    }

    async fn ready(&self) -> Result<RequestHandle, CoreError> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(invalid("Call transport unavailable"));
            }
            if let Some(channel) = self.channel(None)? {
                return Ok(channel);
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + Duration::from_millis(40)).min(deadline),
            )
            .await;
        }
    }
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
        let allocation = Allocation {
            client: self,
            identity: &expected_identity_id,
            device: &expected_device_id,
            space: &space_id,
            peer: &peer_identity_id,
            relay: scope(
                self,
                &expected_identity_id,
                &expected_device_id,
                &space_id,
                &peer_identity_id,
            )?,
        };
        // Transport is allocated before inviting. Unrelated queued messages,
        // profile work or native uploads cannot delay an authenticated pair.
        // Canonical call/selected-device consent remains the RTC/MLS contract.
        let channel = allocation.ready().await?;
        let response = tokio::time::timeout(
            Duration::from_secs(8),
            channel.clone().send(Op::CallTurn {
                space: space_id.clone(),
                peer: peer_identity_id.clone(),
            }),
        )
        .await;
        allocation.channel(Some(&channel))?;
        let Reply::CallTurn(credentials) = response
            .map_err(|_| invalid("Call transport unavailable"))?
            .map_err(net_err)?
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
