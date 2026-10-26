//! Enrollment-backed owner/device/relay binding shared by lifecycle and behavior edits.
use crate::{api::invalid, Client, CoreError, Engine};
use protocol::{Op, Reply};
use serde::{Deserialize, Serialize};
type R<T> = Result<T, CoreError>;
fn changed() -> CoreError {
    invalid("The account, device or agent changed. Review it again.")
}
pub(crate) struct OwnerTarget {
    pub agent: String,
    pub identity: String,
    pub device: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerScope {
    pub(crate) owner: String,
    pub(crate) device: String,
    pub(crate) certificate_sha256: String,
    pub(crate) relay: String,
    pub(crate) agent: String,
    pub(crate) configured_relay: String,
}
impl OwnerScope {
    pub(crate) fn capture(engine: &Engine, target: &OwnerTarget) -> R<Self> {
        let owner =
            crate::agent_enrollment::owner_context(engine, &target.identity, &target.device)?;
        if target.agent == target.identity
            || event_log::agent_owner::enrollment_offer_message(&target.agent, &"0".repeat(64))
                .is_none()
        {
            return Err(changed());
        }
        Ok(Self {
            owner: target.identity.clone(),
            device: target.device.clone(),
            certificate_sha256: owner.certificate_sha256,
            relay: owner.relay,
            agent: target.agent.clone(),
            configured_relay: engine.account().ok_or_else(changed)?.relay_url.clone(),
        })
    }
    pub(crate) fn check(&self, engine: &Engine) -> R<()> {
        let current = Self::capture(
            engine,
            &OwnerTarget {
                agent: self.agent.clone(),
                identity: self.owner.clone(),
                device: self.device.clone(),
            },
        )?;
        if current != *self {
            return Err(changed());
        }
        Ok(())
    }
}
impl Client {
    pub(crate) async fn owner_agent_request(&self, scope: &OwnerScope, op: Op) -> R<Reply> {
        // Match start_sync's net→engine lock order and select the authenticated
        // connection while the reviewed owner/device/relay are still current.
        let request = {
            let net = match self.net.lock() {
                Ok(net) => net,
                Err(poisoned) => poisoned.into_inner(),
            };
            scope.check(&self.lock())?;
            let net = net.as_ref().ok_or_else(changed)?;
            if net
                .authenticated_relay_name(&scope.configured_relay, &scope.owner, &scope.device)
                .is_none()
            {
                return Err(changed());
            }
            net.request_handle()
        };
        request.send(op).await.map_err(invalid)
    }
}
