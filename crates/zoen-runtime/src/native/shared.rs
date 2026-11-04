//! Native capsule/cursor commitments share the existing owner-policy anchor.
//! These maintenance operations grant no model, output or refund authority.
mod discovery;
mod recovery;
use super::*;
use crate::continuity::{inventory, ManagedRuntime};
use crate::native::runs::Batch;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use discovery::Discovery;
use foundationdb::{RangeOption, Transaction};
pub(crate) use recovery::recover_shared_native;
use relay_admission::managed_images::{Prepared, Recovery, SubjectUpdate};

const PREFIX: &str = "native_v1_";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SharedSource {
    pub(super) cell: String,
    pub(super) space: String,
    pub(super) expected: types::Seen,
    pub(super) through: types::Seen,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    format: u8,
    previous: Option<Root>,
    target: Root,
    source: Option<SharedSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discovery: Option<Discovery>,
}
impl Intent {
    fn subject(&self) -> Result<String, RuntimeError> {
        root_subject(&self.target)
    }
    fn effect(&self) -> Result<String, RuntimeError> {
        Ok(hash(
            &serde_json::to_vec(&("runtime-native-maintenance/1", self))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    fn seal(&self, runtime: &RuntimeAuthority) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime.custody.seal("runtime-native-maintenance/1", self)?;
        let encoded = format!("{PREFIX}{}", STANDARD.encode(bytes));
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(encoded)
    }
    fn updates(&self) -> Result<Vec<SubjectUpdate>, RuntimeError> {
        let mut updates = vec![SubjectUpdate {
            subject: self.subject()?,
            previous: self.previous.as_ref().map(root_digest).transpose()?,
            next: root_digest(&self.target)?,
        }];
        if let Some(discovery) = &self.discovery {
            updates.push(discovery.update(&self.target)?);
        }
        Ok(updates)
    }
    fn staged(&self) -> storage::Staged {
        storage::Staged {
            previous: self.previous.clone(),
            root: self.target.clone(),
        }
    }
    fn scope(
        &self,
        runtime: &RuntimeAuthority,
        fence: &crate::execution::Fence,
    ) -> Result<ActivationScope, RuntimeError> {
        let mut scope = self.staged().scope(&runtime.execution, fence);
        if let Some(source) = &self.source {
            scope.effects = hash(
                &serde_json::to_vec(&(
                    &source.cell,
                    &source.space,
                    &source.expected,
                    &source.through,
                ))
                .map_err(|_| RuntimeError::InvalidBinding)?,
            );
        }
        if self.discovery.is_some() {
            scope.effects = self.effect()?;
        }
        Ok(scope)
    }
}
fn root_subject(root: &Root) -> Result<String, RuntimeError> {
    let principal = &root.context.principal;
    Ok(hash(
        &serde_json::to_vec(&("runtime-native-root/1", &principal.agent, &principal.device))
            .map_err(|_| RuntimeError::InvalidBinding)?,
    ))
}
fn root_digest(root: &Root) -> Result<String, RuntimeError> {
    root.context
        .validate(&root.context.principal, root.context.key_version)?;
    if !digest(&root.manifest_digest) {
        return Err(RuntimeError::InvalidBinding);
    }
    Ok(hash(
        &serde_json::to_vec(root).map_err(|_| RuntimeError::InvalidBinding)?,
    ))
}
pub(crate) fn shared_intent(prepared: &Prepared) -> bool {
    prepared
        .intent
        .as_ref()
        .is_some_and(|intent| intent.starts_with(PREFIX))
}
pub(crate) async fn shared_commitments(
    runtime: &RuntimeAuthority,
    trx: &Transaction,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let prefix = runtime.execution.root.subspace(&("native-device",));
    let rows = storage::bounded_rows(trx, RangeOption::from(prefix.range()), 2049, 16_384).await?;
    if rows.len() > 2048 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let mut commitments = BTreeMap::new();
    for (key, value) in &rows {
        let suffix = key
            .as_slice()
            .strip_prefix(prefix.bytes())
            .ok_or(RuntimeError::InvalidBinding)?;
        let (agent, device, part): (String, String, String) =
            foundationdb::tuple::unpack(suffix).map_err(|_| RuntimeError::InvalidBinding)?;
        if part != "root" {
            continue;
        }
        if value.as_slice().len() > 8192 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let root: Root =
            serde_json::from_slice(value.as_slice()).map_err(|_| RuntimeError::InvalidBinding)?;
        if root.context.principal.agent != agent
            || root.context.principal.device != device
            || serde_json::to_vec(&root)
                .map_err(|_| RuntimeError::InvalidBinding)?
                .as_slice()
                != value.as_slice()
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if commitments
            .insert(root_subject(&root)?, root_digest(&root)?)
            .is_some()
        {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    for (subject, value) in discovery::commitments(runtime, trx).await? {
        if commitments.insert(subject, value).is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    Ok(commitments)
}
impl RuntimeAuthority {
    pub(crate) async fn check_shared_runtime(&self) -> Result<(), RuntimeError> {
        if let Some(managed) = &self.runtime_continuity {
            Box::pin(managed.lock().await.reconcile(self)).await?;
        } else {
            let mut sql = self.finance.continuity_transaction().await?;
            if crate::finance::Finance::policy_scope(&mut sql).await? == Some(3) {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            sql.commit().await?;
        }
        Ok(())
    }
}
enum NativeEffect {
    Maintenance(Option<SharedSource>),
    Discovery(Batch),
}
impl NativeCustody {
    pub(super) async fn activate_maintenance(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        staged: storage::Staged,
        fence: &crate::execution::Fence,
        source: Option<SharedSource>,
    ) -> Result<Root, RuntimeError> {
        self.activate_native(
            runtime,
            credential,
            staged,
            fence,
            NativeEffect::Maintenance(source),
        )
        .await
        .map(|(root, _)| root)
    }
    pub(super) async fn activate_discovery(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        staged: storage::Staged,
        fence: &crate::execution::Fence,
        batch: Batch,
    ) -> Result<(Root, Vec<crate::ReplyRun>), RuntimeError> {
        batch.scope(&runtime.execution, &staged, fence)?;
        self.activate_native(
            runtime,
            credential,
            staged,
            fence,
            NativeEffect::Discovery(batch),
        )
        .await
    }
    async fn activate_native(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        staged: storage::Staged,
        fence: &crate::execution::Fence,
        effect: NativeEffect,
    ) -> Result<(Root, Vec<crate::ReplyRun>), RuntimeError> {
        let (source, batch) = match effect {
            NativeEffect::Maintenance(source) => (source, None),
            NativeEffect::Discovery(batch) => (
                Some(SharedSource {
                    cell: batch.source.clone(),
                    space: batch.space.clone(),
                    expected: batch.head.clone(),
                    through: batch.through.clone(),
                }),
                Some(batch),
            ),
        };
        let mut intent = Intent {
            format: 1,
            previous: staged.previous.clone(),
            target: staged.root.clone(),
            source,
            discovery: None,
        };
        let Some(shared) = &runtime.runtime_continuity else {
            if batch.is_some() {
                return Err(RuntimeError::CoreAuthorityUnavailable);
            }
            let scope = intent.scope(runtime, fence)?;
            let admission = runtime
                .finance
                .native_directory(
                    &credential.agent,
                    &credential.owner,
                    &credential.principal().device,
                    &credential.certificate,
                )
                .await?
                .authorize(scope.clone())
                .await?;
            let tx = runtime.execution.transaction().await?;
            verify_source(runtime, &tx, intent.source.as_ref(), true).await?;
            runtime
                .execution
                .native_activation(&tx, &staged, fence)
                .await?;
            admission.consume(&scope)?;
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            return Ok((staged.root, Vec::new()));
        };
        let mut managed = shared.lock().await;
        Box::pin(managed.reconcile(runtime)).await?;
        let mut sql = runtime.finance.continuity_transaction().await?;
        let previous = managed.sql_checkpoint(&mut sql).await?;
        let tx = runtime.execution.transaction().await?;
        if managed.fdb_checkpoint(runtime, &tx).await? != previous
            || managed
                .anchor
                .reader()
                .checkpoint()
                .map_err(crate::managed_images::error)?
                != previous
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let before = inventory(runtime, &mut sql, &tx).await?;
        if before
            != managed
                .anchor
                .reader()
                .subject_commitments()
                .map_err(crate::managed_images::error)?
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let subject = intent.subject()?;
        if before.get(&subject)
            != intent
                .previous
                .as_ref()
                .map(root_digest)
                .transpose()?
                .as_ref()
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if let Some(batch) = batch {
            let discovery = Discovery::retain(runtime, &staged.root, batch).await?;
            discovery.require_capacity(runtime, &tx).await?;
            intent.discovery = Some(discovery);
        }
        let scope = intent.scope(runtime, fence)?;
        let updates = intent.updates()?;
        if updates
            .iter()
            .skip(1)
            .any(|update| before.contains_key(&update.subject))
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let sealed = intent.seal(runtime)?;
        let prepared = managed
            .anchor
            .prepare_subjects(
                &intent.effect()?,
                &previous,
                &previous,
                false,
                &updates,
                Some(&sealed),
            )
            .map_err(crate::managed_images::error)?;
        drop(sql);
        runtime
            .execution
            .pin_image_activation(&tx, &staged, fence, sealed.as_bytes())
            .await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 90 {
            return Err(RuntimeError::Unavailable);
        }
        let admission = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &credential.principal().device,
                &credential.certificate,
            )
            .await?
            .shared_native_checkpoint(&managed, &prepared, &scope, false)
            .await?
            .authorize(scope.clone())
            .await?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 91 {
            return Err(RuntimeError::Unavailable);
        }
        let tx = runtime.execution.transaction().await?;
        verify_source(runtime, &tx, intent.source.as_ref(), true).await?;
        runtime
            .execution
            .native_activation(&tx, &staged, fence)
            .await?;
        let runs = if let Some(discovery) = &intent.discovery {
            discovery
                .apply(runtime, &tx, &staged.root, true)
                .await?
                .runs
        } else {
            Vec::new()
        };
        managed.write_fdb(runtime, &tx, &prepared).await?;
        tx.clear(
            &runtime
                .execution
                .image_activation_pin(&credential.principal()),
        );
        admission.consume(&scope)?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 92 {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(managed.reconcile(runtime)).await?;
        Ok((staged.root, runs))
    }
}
async fn verify_source(
    runtime: &RuntimeAuthority,
    tx: &Transaction,
    source: Option<&SharedSource>,
    latest: bool,
) -> Result<(), RuntimeError> {
    let Some(source) = source else {
        return Ok(());
    };
    let actual = runtime.execution.reply_source()?;
    if source.cell != actual.cell() {
        return Err(RuntimeError::DeploymentMismatch);
    }
    let position = actual
        .position_in(tx, &source.space, source.through.seq)
        .await
        .map_err(|_| RuntimeError::Denied)?;
    if position.hash != source.through.hash {
        return Err(RuntimeError::InvalidBinding);
    }
    if latest {
        let head = actual
            .head_in(tx, &source.space)
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if head.seq() != source.expected.seq || head.hash() != source.expected.hash {
            return Err(RuntimeError::Denied);
        }
    }
    Ok(())
}
