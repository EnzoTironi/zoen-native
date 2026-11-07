//! Native capsule/cursor commitments share the existing owner-policy anchor.
//! These maintenance operations grant no model, output or refund authority.
mod activation;
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

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publication: Option<runs::output::SharedPublication>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool: Option<runs::reviews::SharedToolDecision>,
}
impl Intent {
    fn subject(&self) -> Result<String, RuntimeError> {
        root_subject(&self.target)
    }
    fn effect(&self) -> Result<String, RuntimeError> {
        if let Some(publication) = &self.publication {
            if self.discovery.is_some()
                || self.tool.is_some()
                || self.source.as_ref() != Some(publication.source())
            {
                return Err(RuntimeError::InvalidBinding);
            }
            return publication.effect();
        }
        if let Some(tool) = &self.tool {
            if self.discovery.is_some() || self.source.as_ref() != Some(tool.source()) {
                return Err(RuntimeError::InvalidBinding);
            }
            return tool.effect();
        }
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
        if let Some(publication) = &self.publication {
            updates.push(publication.update().clone());
        }
        if let Some(tool) = &self.tool {
            updates.push(tool.update().clone());
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
        if let Some(publication) = &self.publication {
            scope.effects = publication.digest()?;
        }
        if let Some(tool) = &self.tool {
            scope.effects = tool.digest()?;
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
fn open_intent(custody: &Custody, prepared: &Prepared) -> Result<Intent, RuntimeError> {
    let encoded = prepared
        .intent
        .as_deref()
        .and_then(|value| value.strip_prefix(PREFIX))
        .ok_or(RuntimeError::RestoreUnreconciled)?;
    if encoded.len() > 16_384 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| RuntimeError::InvalidBinding)?;
    let intent: Intent = custody.open("runtime-native-maintenance/1", None, &bytes)?;
    if intent.format != 1
        || prepared.retirement
        || prepared.effect_digest != intent.effect()?
        || prepared.subject_updates != intent.updates()?
    {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    Ok(intent)
}
pub(in crate::native) fn publication_transition(
    custody: &Custody,
    prepared: &Prepared,
    run: &str,
    bytes: &[u8],
) -> Result<(), RuntimeError> {
    let intent = open_intent(custody, prepared)?;
    let publication = intent
        .publication
        .ok_or(RuntimeError::RestoreUnreconciled)?;
    publication.matches_original(run, bytes, prepared.next.sequence)
}
pub(in crate::native) fn tool_transition(
    custody: &Custody,
    prepared: &Prepared,
    run: &str,
    index: u8,
    bytes: &[u8],
) -> Result<(), RuntimeError> {
    open_intent(custody, prepared)?
        .tool
        .ok_or(RuntimeError::RestoreUnreconciled)?
        .matches_original(run, index, bytes, prepared.next.sequence)
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
    for (subject, value) in super::cancel_commitments(runtime, trx).await? {
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

#[cfg(test)]
impl ManagedRuntime {
    pub(crate) fn test_pending_publication(
        &self,
        runtime: &RuntimeAuthority,
    ) -> Result<(String, String), RuntimeError> {
        let prepared = self
            .anchor
            .pending()
            .map_err(crate::managed_images::error)?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let intent = open_intent(&runtime.custody, &prepared)?;
        let publication = intent
            .publication
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        Ok((prepared.effect_digest, publication.digest()?))
    }
}
