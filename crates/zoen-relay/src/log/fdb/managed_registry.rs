//! Registry ownership is process/operator configuration, never a restored row.
use super::*;
use relay_admission::managed_images::{Checkpoint, Deployment};

impl FdbLog {
    pub fn configure_managed_devices(
        &self,
        devices: crate::managed_devices::ManagedDevices,
    ) -> anyhow::Result<()> {
        let (prefix, cell): (String, String) = unpack(self.cell.root.bytes())?;
        anyhow::ensure!(
            prefix == "zoen" && devices.source_matches(&cell),
            "managed cell mismatch"
        );
        anyhow::ensure!(
            self.forwarding.get().is_none() || devices.is_empty(),
            "managed devices require a single host"
        );
        self.managed_devices
            .set(devices)
            .map_err(|_| anyhow::anyhow!("managed registry already configured"))
    }

    pub(super) async fn managed_store_checkpoint(
        &self,
        deployment: &Deployment,
    ) -> Result<Option<Checkpoint>, StoreError> {
        let invalid = || StoreError("managed deployment refused".into());
        let (prefix, cell): (String, String) =
            unpack(self.cell.root.bytes()).map_err(|_| invalid())?;
        if prefix != "zoen"
            || cell != deployment.source
            || !protocol::runtime_binding::valid_namespace(&deployment.namespace)
        {
            return Ok(None);
        }
        let root = Subspace::all().subspace(&("zoen-runtime-v1", deployment.namespace.as_str()));
        let transaction = self.cell.db.create_trx().map_err(|_| invalid())?;
        transaction
            .set_option(foundationdb::options::TransactionOption::Timeout(2000))
            .map_err(|_| invalid())?;
        let witness = transaction
            .get(&root.pack(&("deployment",)), false)
            .await
            .map_err(|_| invalid())?;
        let source = transaction
            .get(&root.pack(&("relay-source/1",)), false)
            .await
            .map_err(|_| invalid())?;
        if witness.as_deref() != Some(deployment.witness.as_bytes())
            || source.as_deref() != Some(deployment.source.as_bytes())
        {
            return Ok(None);
        }
        let value = transaction
            .get(&root.pack(&("managed-images/checkpoint/1",)), false)
            .await
            .map_err(|_| invalid())?;
        let Some(value) = value else { return Ok(None) };
        if value.len() > 4096 {
            return Err(invalid());
        }
        serde_json::from_slice(&value)
            .map(Some)
            .map_err(|_| invalid())
    }
}
