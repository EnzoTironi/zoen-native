//! An authenticated Images intent pins its original stage. Recovery still needs
//! a new directory admission and a newly acquired device lease.
use super::*;

impl Execution {
    pub(in crate::native) fn image_activation_pin(&self, principal: &Principal) -> Vec<u8> {
        self.native_key(principal, "image-activation/1")
    }

    pub(in crate::native) async fn pin_image_activation(
        &self,
        trx: &Transaction,
        staged: &Staged,
        fence: &Fence,
        intent: &[u8],
    ) -> Result<(), RuntimeError> {
        let stage = self.stage_scope(trx, staged, fence).await?;
        if stage.state != StageState::Ready {
            return Err(RuntimeError::Denied);
        }
        let key = self.image_activation_pin(&staged.root.context.principal);
        if trx.get(&key, false).await?.is_some() {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        trx.set(&key, intent);
        Ok(())
    }

    pub(in crate::native) async fn recover_image_stage(
        &self,
        trx: &Transaction,
        staged: &Staged,
        fence: &Fence,
    ) -> Result<PreparedImage, RuntimeError> {
        let principal = &staged.root.context.principal;
        let version = trx.get_read_version().await?;
        Self::lease(trx, &self.native_lease(principal), fence, version).await?;
        if self.native_root(trx, principal).await? != staged.previous {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let key = self.native_key(principal, "stage");
        let mut stage: Stage = decode(
            &trx.get(&key, false)
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?,
        )?;
        if stage.previous != staged.previous
            || stage.root != staged.root
            || stage.state != StageState::Ready
            || stage.token >= fence.token
            || stage.holder == fence.holder
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let generation = self.native_generation(&staged.root);
        let manifest = trx
            .get(&generation.pack(&("manifest",)), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if !(40..MANIFEST_BYTES).contains(&manifest.len())
            || hash(&manifest) != staged.root.manifest_digest
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let mut chunks = Vec::new();
        for index in 0..staged.root.context.chunks {
            let bytes = trx
                .get(&generation.pack(&("chunk", i64::from(index))), false)
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?;
            if !(41..=CHUNK_BYTES + 40).contains(&bytes.len()) {
                return Err(RuntimeError::InvalidBinding);
            }
            chunks.push(bytes.to_vec());
        }
        stage.holder = fence.holder.clone();
        stage.token = fence.token;
        stage.expires = version
            .checked_add(STAGE_VERSIONS)
            .ok_or(RuntimeError::Unavailable)?;
        trx.set(&key, &encode(&stage)?);
        Ok(PreparedImage {
            root: staged.root.clone(),
            manifest: manifest.to_vec(),
            chunks,
        })
    }
}
