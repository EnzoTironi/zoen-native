//! Resolve transport facts from existing private authorities. A matching pair
//! proves neither cross-store restore continuity nor permission for an effect.
use client_core::relay_output_transport::RelayOutputTransport;
use event_log::Author;
use protocol::runtime_binding::{Expectation, Request};

use crate::{RuntimeAuthority, RuntimeError};

impl RuntimeAuthority {
    pub(crate) async fn connect_bound_output(
        &self,
        url: &str,
        relay: &str,
        actor: Author,
    ) -> Result<RelayOutputTransport, RuntimeError> {
        let source = self
            .execution
            .source
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        let (namespace, witness): (String, String) = sqlx::query_as(
            "SELECT namespace,witness FROM runtime_deployment_binding WHERE singleton",
        )
        .fetch_one(&self.finance.pool)
        .await?;
        if witness != self.execution.deployment
            || self.execution.root.bytes()
                != foundationdb::tuple::pack(&("zoen-runtime-v1", namespace.as_str()))
        {
            return Err(RuntimeError::DeploymentMismatch);
        }
        // Read both original markers in one actual FDB transaction. No public
        // constructor accepts caller namespace, cell or a financial capability.
        let trx = self.execution.transaction().await?;
        let observed = trx
            .get(&self.execution.root.pack(&("relay-source/1",)), false)
            .await?;
        if observed.as_deref() != Some(source.cell().as_bytes()) {
            return Err(RuntimeError::DeploymentMismatch);
        }
        let mut nonce = vec![0u8; 32];
        getrandom::getrandom(&mut nonce).map_err(|_| RuntimeError::Unavailable)?;
        let expected = Expectation::new(
            &witness,
            Request { namespace, nonce },
            source.cell(),
            relay,
            &actor.identity,
            &actor.key.id(),
        )
        .ok_or(RuntimeError::InvalidBinding)?;
        RelayOutputTransport::connect_bound(url, relay, actor, expected)
            .await
            .map_err(|error| match error {
                client_core::relay_output_transport::RelayOutputError::Invalid => {
                    RuntimeError::InvalidBinding
                }
                client_core::relay_output_transport::RelayOutputError::Refused => {
                    RuntimeError::Denied
                }
                _ => RuntimeError::Unavailable,
            })
    }
}
