//! Forward-only recovery of the original Images maintenance activation. The
//! encrypted intent is inert; a current SQL directory admission is still needed.
#[cfg(test)]
mod testing;
use super::*;
use crate::{execution::Fence, managed_images::PublishCheckpoint};
use foundationdb::Transaction;
use protocol::{ClientFrame, Envelope};
use relay_admission::managed_images::{Deployment, Prepared};
use storage::Staged;
use types::Seen;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum ImageActivationEffect {
    Review { key: Vec<u8> },
    Output { run: String },
}

impl ImageActivationEffect {
    fn location(&self, runtime: &RuntimeAuthority) -> Vec<u8> {
        match self {
            Self::Review { key } => key.clone(),
            Self::Output { run } => runtime.execution.run(run, "image-output/1"),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    prepared: Prepared,
    previous: Root,
    target: Root,
    effect: ImageActivationEffect,
    prior: Option<String>,
    record: String,
    wire: String,
    space: String,
    head: Seen,
    expires: Option<i64>,
}

fn subject(deployment: &Deployment) -> String {
    format!(
        "managed-image-activation/1/{}/{}/{}",
        deployment.namespace, deployment.agent, deployment.device
    )
}
fn data_key(runtime: &RuntimeAuthority, prepared: &Prepared, part: &str) -> Vec<u8> {
    runtime.execution.root.pack(&(
        "managed-images/activation-data/1",
        prepared.next.digest.as_str(),
        part,
    ))
}

pub(super) struct ImageActivationRecord<'a> {
    pub(super) effect: ImageActivationEffect,
    pub(super) prior: Option<String>,
    pub(super) bytes: &'a [u8],
    pub(super) envelope: &'a Envelope,
    pub(super) head: Seen,
    pub(super) expires: Option<i64>,
}

impl RuntimeAuthority {
    pub(super) async fn retain_image_activation(
        &self,
        transaction: &Transaction,
        checkpoint: &PublishCheckpoint,
        staged: &Staged,
        fence: &Fence,
        record: ImageActivationRecord<'_>,
    ) -> Result<(), RuntimeError> {
        let prepared = checkpoint.prepared();
        let deployment = checkpoint.deployment();
        let wire = ClientFrame::Publish {
            env: record.envelope.clone(),
        }
        .encode();
        if record.bytes.len() > 98_304
            || wire.len() > 98_304
            || relay_admission::managed_images::publish_digest(
                &deployment.namespace,
                record.envelope,
            )
            .map_err(crate::managed_images::error)?
                != prepared.effect_digest
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let intent = Intent {
            prepared: prepared.clone(),
            previous: staged
                .previous
                .clone()
                .ok_or(RuntimeError::InvalidBinding)?,
            target: staged.root.clone(),
            effect: record.effect,
            prior: record.prior,
            record: hash(record.bytes),
            wire: hash(&wire),
            space: record.envelope.space().into(),
            head: record.head,
            expires: record.expires,
        };
        let (_, sealed) = self.custody.seal(&subject(deployment), &intent)?;
        if sealed.len() > 8192 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        self.execution
            .pin_image_activation(transaction, staged, fence, &sealed)
            .await?;
        for (part, bytes) in [("record", record.bytes), ("wire", wire.as_slice())] {
            let key = data_key(self, prepared, part);
            if transaction.get(&key, false).await?.is_some() {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            transaction.set(&key, bytes);
        }
        Ok(())
    }

    pub(super) fn clear_image_activation(
        &self,
        transaction: &Transaction,
        checkpoint: &PublishCheckpoint,
    ) {
        let deployment = checkpoint.deployment();
        let pin = self.execution.root.pack(&(
            "native-device",
            deployment.agent.as_str(),
            deployment.device.as_str(),
            "image-activation/1",
        ));
        transaction.clear(&pin);
        for part in ["record", "wire"] {
            transaction.clear(&data_key(self, checkpoint.prepared(), part));
        }
    }

    /// None means there is no retained activation for this pending transition;
    /// it does not turn an unproven checkpoint into a directory admission.
    pub(crate) async fn recover_image_activation(
        &self,
        deployment: &Deployment,
        prepared: &Prepared,
    ) -> Result<bool, RuntimeError> {
        let transaction = self.execution.transaction().await?;
        let pin = self.execution.root.pack(&(
            "native-device",
            deployment.agent.as_str(),
            deployment.device.as_str(),
            "image-activation/1",
        ));
        let Some(sealed) = transaction.get(&pin, false).await? else {
            return Ok(false);
        };
        let custody = self
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        let credential = custody.credential(&deployment.agent, &deployment.device)?;
        let principal = credential.principal();
        if sealed.len() > 8192 {
            return Err(RuntimeError::InvalidBinding);
        }
        let intent: Intent = self.custody.open(&subject(deployment), None, &sealed)?;
        if intent.prepared != *prepared
            || prepared.retirement
            || intent.target.context.principal != principal
            || intent.previous.context.principal != principal
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        drop(transaction);
        let fence = self.execution.acquire_native_device(&principal).await?;
        let result = Box::pin(
            self.recover_image_activation_fenced(deployment, credential, &fence, &intent, &sealed),
        )
        .await;
        let release = self
            .execution
            .release_native_device(&principal, &fence)
            .await;
        result?;
        release?;
        Ok(true)
    }

    async fn recover_image_activation_fenced(
        &self,
        deployment: &Deployment,
        credential: &Credential,
        fence: &Fence,
        intent: &Intent,
        sealed: &[u8],
    ) -> Result<(), RuntimeError> {
        let checkpoint =
            PublishCheckpoint::from_activation(deployment.clone(), intent.prepared.clone());
        let staged = Staged {
            previous: Some(intent.previous.clone()),
            root: intent.target.clone(),
        };
        let transaction = self.execution.transaction().await?;
        let pin = self.execution.image_activation_pin(&credential.principal());
        if transaction.get(&pin, false).await?.as_deref() != Some(sealed) {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let record = transaction
            .get(&data_key(self, &intent.prepared, "record"), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let wire = transaction
            .get(&data_key(self, &intent.prepared, "wire"), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if record.len() > 98_304
            || wire.len() > 98_304
            || hash(&record) != intent.record
            || hash(&wire) != intent.wire
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let ClientFrame::Publish { env } =
            ClientFrame::decode(&wire).map_err(|_| RuntimeError::InvalidBinding)?
        else {
            return Err(RuntimeError::InvalidBinding);
        };
        if env.author() != deployment.agent
            || env.device() != Some(deployment.device.as_str())
            || env.space() != intent.space
            || env.cert.as_deref() != Some(credential.certificate.as_str())
            || env.verify().is_err()
            || (ClientFrame::Publish { env: env.clone() }).encode() != wire.as_ref()
            || relay_admission::managed_images::publish_digest(&deployment.namespace, &env)
                .map_err(crate::managed_images::error)?
                != intent.prepared.effect_digest
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let key = intent.effect.location(self);
        let (begin, end) = self.execution.root.range();
        if key < begin || key >= end {
            return Err(RuntimeError::InvalidBinding);
        }
        if transaction
            .get(&key, false)
            .await?
            .as_ref()
            .map(|bytes| hash(bytes))
            != intent.prior
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let head = self
            .execution
            .reply_source()?
            .head_in(&transaction, &intent.space)
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if head.seq() != intent.head.seq || head.hash() != intent.head.hash {
            return Err(RuntimeError::Denied);
        }
        let image = self
            .execution
            .recover_image_stage(&transaction, &staged, fence)
            .await?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .open_image(&credential.principal(), image)?;
        let mut scope = staged.scope(&self.execution, fence);
        scope.effects = intent.record.clone();
        let admission = self
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &deployment.device,
                &credential.certificate,
            )
            .await?
            .image_recovery_checkpoint(&checkpoint)
            .await?
            .authorize_before(scope.clone(), intent.expires)
            .await?;
        admission.check(&scope)?;
        self.execution
            .native_activation(&transaction, &staged, fence)
            .await?;
        transaction.set(&key, &record);
        checkpoint.write_fdb(&self.execution, &transaction).await?;
        self.clear_image_activation(&transaction, &checkpoint);
        admission.consume(&scope)?;
        transaction
            .commit()
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
}
