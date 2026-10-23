//! A common pre-send crash consumes the original claim and commits FDB admission,
//! then loses the SQL finish. Recover only the checkpoint observation. A read
//! must never manufacture FreshAdmission, DispatchPermit or a financial witness.
use super::*;
use foundationdb::Transaction;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    prepared: Prepared,
    reference: DispatchReference,
}

fn reference_key(execution: &crate::execution::Execution, sequence: u64) -> Vec<u8> {
    execution
        .root
        .pack(&("managed-images/dispatch/1", sequence))
}

impl DispatchCheckpoint {
    pub(super) async fn write_dispatch_reference(
        &self,
        execution: &crate::execution::Execution,
        transaction: &Transaction,
    ) -> Result<(), RuntimeError> {
        if self.prepared.retirement || self.reference.digest()? != self.prepared.effect_digest {
            return Err(RuntimeError::InvalidBinding);
        }
        let key = reference_key(execution, self.prepared.next.sequence);
        if transaction.get(&key, false).await?.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        let evidence = Evidence {
            prepared: self.prepared.clone(),
            reference: self.reference.clone(),
        };
        transaction.set(
            &key,
            &serde_json::to_vec(&evidence).map_err(|_| RuntimeError::InvalidBinding)?,
        );
        Ok(())
    }
}

impl RuntimeAuthority {
    pub(super) async fn reconcile_image_anchor(
        &self,
        anchor: &mut AnchorWriter,
        deployment: &Deployment,
    ) -> Result<(), RuntimeError> {
        let (sql, fdb) = self.image_checkpoints(deployment).await?;
        // Publication activation commits SQL first. A retained inert intent can
        // be reauthorized as maintenance; dispatch recovery below is separate.
        if let Some(prepared) = anchor.pending().map_err(error)? {
            if !prepared.retirement
                && fdb == prepared.previous
                && (sql == prepared.previous || sql == prepared.next)
                && Box::pin(self.recover_image_activation(deployment, &prepared)).await?
            {
                let (sql, fdb) = self.image_checkpoints(deployment).await?;
                if sql != prepared.next
                    || fdb != prepared.next
                    || anchor.reconcile(&sql, &fdb).map_err(error)? != Recovery::BothAfter
                {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
                return Ok(());
            }
        }
        match anchor.reconcile(&sql, &fdb).map_err(error)? {
            Recovery::Clean | Recovery::BothBefore | Recovery::BothAfter => return Ok(()),
            Recovery::FdbBefore => return Err(RuntimeError::RestoreUnreconciled),
            Recovery::SqlBefore => {}
        }
        let prepared = anchor
            .pending()
            .map_err(error)?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if prepared.retirement {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let transaction = self.execution.transaction().await?;
        let bytes = transaction
            .get(
                &reference_key(&self.execution, prepared.next.sequence),
                false,
            )
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if bytes.len() > 4096 {
            return Err(RuntimeError::InvalidBinding);
        }
        let evidence: Evidence =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if evidence.prepared != prepared || evidence.reference.digest()? != prepared.effect_digest {
            return Err(RuntimeError::InvalidBinding);
        }
        let checkpoint_bytes = transaction
            .get(&stores::key(&self.execution), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let observed: Checkpoint =
            serde_json::from_slice(&checkpoint_bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        let original = &evidence.reference;
        let admitted = transaction
            .get(
                &self.execution.attempt(&original.attempt, "admitted"),
                false,
            )
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let sealed = transaction
            .get(
                &self.execution.attempt(&original.attempt, "prepared"),
                false,
            )
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let binding: Binding =
            self.custody
                .open(&format!("prepared/{}", original.attempt), None, &sealed)?;
        if observed != prepared.next
            || admitted.as_ref() != original.nonce.as_bytes()
            || binding.digest()? != original.binding_digest
            || binding.context.attempt_id != original.attempt
            || binding.context.agent != deployment.agent
            || binding.context.device != deployment.device
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let annotation = DispatchCheckpoint {
            deployment: deployment.clone(),
            prepared,
            reference: evidence.reference,
        };
        self.finance
            .reconcile_image_claim_checkpoint(&annotation)
            .await?;
        let (sql, fdb) = self.image_checkpoints(deployment).await?;
        if anchor.reconcile(&sql, &fdb).map_err(error)? != Recovery::BothAfter {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
}
