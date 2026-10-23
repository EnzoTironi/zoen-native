//! Resolve transport facts from existing private authorities. A matching pair
//! proves neither cross-store restore continuity nor permission for an effect.
use client_core::relay_output_transport::RelayOutputTransport;
use event_log::Author;
use protocol::runtime_binding::{Expectation, ManagedCheckpoint, Request};

use crate::{RuntimeAuthority, RuntimeError};

impl RuntimeAuthority {
    pub(crate) async fn connect_bound_output(
        &self,
        url: &str,
        relay: &str,
        actor: Author,
    ) -> Result<RelayOutputTransport, RuntimeError> {
        let expected = self
            .bound_output_expectation(relay, &actor.identity, &actor.key.id(), None)
            .await?;
        Self::connect_output_expectation(url, relay, actor, expected).await
    }

    pub(crate) async fn connect_bound_image_output(
        &self,
        url: &str,
        relay: &str,
        actor: Author,
        envelope: &protocol::Envelope,
        checkpoint: Option<&relay_admission::managed_images::Checkpoint>,
    ) -> Result<RelayOutputTransport, RuntimeError> {
        let Some(expected) = self
            .image_publish_expectation(relay, envelope, checkpoint)
            .await?
        else {
            return self.connect_bound_output(url, relay, actor).await;
        };
        Self::connect_output_expectation(url, relay, actor, expected).await
    }

    pub(crate) async fn image_publish_expectation(
        &self,
        relay: &str,
        envelope: &protocol::Envelope,
        checkpoint: Option<&relay_admission::managed_images::Checkpoint>,
    ) -> Result<Option<Expectation>, RuntimeError> {
        self.validate_image_publish_checkpoint(envelope, checkpoint)
            .await?;
        let Some(checkpoint) = checkpoint else {
            return Ok(None);
        };
        let managed = ManagedCheckpoint {
            generation: checkpoint.generation,
            sequence: checkpoint.sequence,
            digest: checkpoint.digest.clone(),
        };
        self.bound_output_expectation(
            relay,
            envelope.author(),
            envelope.device().ok_or(RuntimeError::InvalidBinding)?,
            Some(managed),
        )
        .await
        .map(Some)
    }

    async fn bound_output_expectation(
        &self,
        relay: &str,
        identity: &str,
        device: &str,
        managed: Option<ManagedCheckpoint>,
    ) -> Result<Expectation, RuntimeError> {
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
        Expectation::new(
            &witness,
            Request {
                namespace,
                nonce,
                managed,
            },
            source.cell(),
            relay,
            identity,
            device,
        )
        .ok_or(RuntimeError::InvalidBinding)
    }

    async fn connect_output_expectation(
        url: &str,
        relay: &str,
        actor: Author,
        expected: Expectation,
    ) -> Result<RelayOutputTransport, RuntimeError> {
        RelayOutputTransport::connect_bound(url, relay, actor, expected)
            .await
            .map_err(|error| match error {
                client_core::relay_output_transport::RelayOutputError::Unknown => {
                    RuntimeError::RestoreUnreconciled
                }
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
