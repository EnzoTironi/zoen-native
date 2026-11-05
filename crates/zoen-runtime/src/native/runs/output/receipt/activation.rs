//! Shared and unmanaged receipt activation use the same transaction proof.
use super::*;
impl Execution {
    pub(super) async fn activate_receipt(
        &self,
        capability: &dyn NativeOutputAuthority,
        activation: receipt::ReceiptActivation<'_>,
        runtime: &RuntimeAuthority,
    ) -> Result<ReplyReceiptProgress, RuntimeError> {
        if runtime.runtime_continuity.is_some() {
            return Box::pin(shared::receipt::activate(runtime, capability, activation)).await;
        }
        let trx = self.transaction().await?;
        let progress = self
            .apply_receipt_in(&trx, capability, &activation, runtime)
            .await?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 48 {
            return Err(RuntimeError::Unavailable);
        }
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 49 {
            return Err(RuntimeError::Unavailable);
        }
        Ok(progress)
    }

    pub(in crate::native::runs::output) async fn apply_receipt_in(
        &self,
        trx: &Transaction,
        capability: &dyn NativeOutputAuthority,
        activation: &receipt::ReceiptActivation<'_>,
        runtime: &RuntimeAuthority,
    ) -> Result<ReplyReceiptProgress, RuntimeError> {
        capability.check_in(self, trx).await?;
        let loaded = activation.loaded;
        self.receipt_scope_in(trx, loaded, &runtime.custody).await?;
        if loaded.receipt.is_some() {
            return Err(RuntimeError::Denied);
        }
        let source = self.reply_source()?;
        let head = source
            .head_in(trx, loaded.record.intent.space())
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if head.seq() != activation.source.seq || head.hash() != activation.source.hash {
            return Err(RuntimeError::Denied);
        }
        let position = source
            .position_in(trx, loaded.record.intent.space(), activation.position.seq)
            .await
            .map_err(|_| RuntimeError::Denied)?;
        if position != activation.position {
            return Err(RuntimeError::InvalidBinding);
        }
        receipt::verify_position(&loaded.publication.value, &activation.proof, &position)?;
        let target = activation.target();
        if target.context.image_digest != hash(activation.image().as_bytes()) {
            return Err(RuntimeError::InvalidBinding);
        }
        let progress = match (&activation.proof, &activation.receipt) {
            (ReplyReceipt::Accepted(proof), Some(record)) => {
                record.value.validate(&loaded.publication)?;
                if record.value.root != *target
                    || record.value.accepted != *proof.accepted()
                    || record.value.source != activation.source
                {
                    return Err(RuntimeError::InvalidBinding);
                }
                trx.set(
                    &self.run(&loaded.record.locator.run, "native-receipt/1"),
                    &record.bytes,
                );
                ReplyReceiptProgress::Accepted {
                    accepted: proof.accepted().clone(),
                    generation: target.context.generation,
                }
            }
            (ReplyReceipt::Prefix(proof), None) if activation.staged.is_some() => {
                ReplyReceiptProgress::CatchingUp {
                    through: proof.frontier().clone(),
                    generation: target.context.generation,
                }
            }
            _ => return Err(RuntimeError::InvalidBinding),
        };
        if let Some(staged) = &activation.staged {
            if staged.previous.as_ref() != Some(&loaded.current) {
                return Err(RuntimeError::InvalidBinding);
            }
            self.native_activation(trx, staged, &loaded.fences.device)
                .await?;
        } else if target != &loaded.current {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(progress)
    }
}
