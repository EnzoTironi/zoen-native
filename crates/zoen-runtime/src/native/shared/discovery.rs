//! Retain the original discovery batch using the existing encrypted evidence
//! chunks. Commitments verify its actual run/trigger/binding/wake/reference rows.
use super::*;
use crate::native::runs::{Batch, DiscoveryMode, ReplyEffects};
const MAXIMUM: usize = 3 * 1024 * 1024;
const INVENTORY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Original {
    batch: Batch,
    checks: Vec<(Vec<u8>, String)>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Discovery {
    payload: String,
    digest: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Commitment {
    target: Root,
    discovery: Discovery,
}
impl Discovery {
    fn payload(&self) -> Result<Vec<u8>, RuntimeError> {
        if self.payload.len() > crate::custody::MAX_SEALED * 4 / 3 + 4 {
            return Err(RuntimeError::InvalidBinding);
        }
        STANDARD
            .decode(&self.payload)
            .map_err(|_| RuntimeError::InvalidBinding)
    }
    fn payload_size(&self) -> Result<usize, RuntimeError> {
        crate::execution::Execution::evidence_payload_size(&self.payload()?)
    }
    fn subject(root: &Root) -> String {
        hash(format!("runtime-native-discovery/1/{}", root.context.capsule).as_bytes())
    }
    fn key(runtime: &RuntimeAuthority, root: &Root) -> Vec<u8> {
        runtime
            .execution
            .root
            .pack(&("shared-native-discovery/1", root.context.capsule.as_str()))
    }
    fn bytes(&self, root: &Root) -> Result<Vec<u8>, RuntimeError> {
        serde_json::to_vec(&Commitment {
            target: root.clone(),
            discovery: self.clone(),
        })
        .map_err(|_| RuntimeError::InvalidBinding)
    }
    pub(super) fn update(&self, root: &Root) -> Result<SubjectUpdate, RuntimeError> {
        Ok(SubjectUpdate {
            subject: Self::subject(root),
            previous: None,
            next: hash(&self.bytes(root)?),
        })
    }
    pub(super) async fn retain(
        runtime: &RuntimeAuthority,
        root: &Root,
        batch: Batch,
    ) -> Result<Self, RuntimeError> {
        // Planning writes are deliberately dropped, never committed before SQL.
        let tx = runtime.execution.transaction().await?;
        let effects = runtime
            .execution
            .reply_effects_in(&tx, root, &batch, &runtime.custody, DiscoveryMode::Current)
            .await?;
        drop(tx);
        let plain = serde_json::to_vec(&Original {
            batch,
            checks: effects.checks,
        })
        .map_err(|_| RuntimeError::InvalidBinding)?;
        let digest = hash(&plain);
        let subject = format!("native-discovery/1/{}", root.context.capsule);
        let (_, sealed) = runtime
            .custody
            .seal_bounded_bytes(&subject, &plain, MAXIMUM)?;
        let payload = runtime
            .execution
            .referenced_evidence_payload(&root.context.capsule, &digest, &sealed)
            .await?;
        Ok(Self {
            payload: STANDARD.encode(payload),
            digest,
        })
    }
    pub(super) async fn require_capacity(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
    ) -> Result<(), RuntimeError> {
        let prefix = runtime
            .execution
            .root
            .subspace(&("shared-native-discovery/1",));
        let rows = storage::bounded_rows(
            tx,
            RangeOption::from(prefix.range()),
            513,
            crate::custody::MAX_SEALED,
        )
        .await?;
        if rows.len() >= 512 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let mut bytes = self.payload_size()?;
        if bytes > INVENTORY_BYTES {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        for (_, value) in rows {
            let commitment: Commitment =
                serde_json::from_slice(&value).map_err(|_| RuntimeError::InvalidBinding)?;
            bytes = bytes
                .checked_add(commitment.discovery.payload_size()?)
                .ok_or(RuntimeError::EvidenceTooLarge)?;
            if bytes > INVENTORY_BYTES {
                return Err(RuntimeError::EvidenceTooLarge);
            }
        }
        Ok(())
    }
    async fn load(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        root: &Root,
    ) -> Result<Original, RuntimeError> {
        if !digest(&self.digest) || self.payload_size()? > MAXIMUM {
            return Err(RuntimeError::InvalidBinding);
        }
        let sealed = runtime
            .execution
            .read_evidence_payload_in(tx, &root.context.capsule, &self.digest, &self.payload()?)
            .await?;
        let subject = format!("native-discovery/1/{}", root.context.capsule);
        // open() retains the ordinary small-record bound; this explicit bounded
        // original uses the same byte codec as larger captured evidence.
        let bytes =
            runtime
                .custody
                .open_bounded_bytes(&subject, Some(&self.digest), &sealed, MAXIMUM)?;
        let original: Original =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if original.checks.len() > 80
            || original.checks.windows(2).any(|v| v[0].0 >= v[1].0)
            || original.checks.iter().any(|(key, value)| {
                key.len() > 2048
                    || !key.starts_with(runtime.execution.root.bytes())
                    || !digest(value)
            })
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(original)
    }
    pub(super) async fn apply(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        root: &Root,
        latest: bool,
    ) -> Result<ReplyEffects, RuntimeError> {
        let original = self.load(runtime, tx, root).await?;
        let effects = runtime
            .execution
            .reply_effects_in(
                tx,
                root,
                &original.batch,
                &runtime.custody,
                if latest {
                    DiscoveryMode::Current
                } else {
                    DiscoveryMode::Recovery
                },
            )
            .await?;
        if effects.checks != original.checks {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let key = Self::key(runtime, root);
        let bytes = self.bytes(root)?;
        if tx
            .get(&key, false)
            .await?
            .as_deref()
            .is_some_and(|prior| prior != bytes)
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        tx.set(&key, &bytes);
        Ok(effects)
    }
}
pub(super) async fn commitments(
    runtime: &RuntimeAuthority,
    tx: &Transaction,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let prefix = runtime
        .execution
        .root
        .subspace(&("shared-native-discovery/1",));
    let rows = storage::bounded_rows(
        tx,
        RangeOption::from(prefix.range()),
        513,
        crate::custody::MAX_SEALED,
    )
    .await?;
    if rows.len() > 512 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let mut result = BTreeMap::new();
    let mut budget = 0usize;
    for (key, bytes) in rows {
        let commitment: Commitment =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        let root = &commitment.target;
        root_digest(root)?;
        if key != Discovery::key(runtime, root) || bytes != commitment.discovery.bytes(root)? {
            return Err(RuntimeError::InvalidBinding);
        }
        budget = budget
            .checked_add(commitment.discovery.payload_size()?)
            .ok_or(RuntimeError::EvidenceTooLarge)?;
        if budget > INVENTORY_BYTES {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let original = commitment.discovery.load(runtime, tx, root).await?;
        for (key, expected) in original.checks {
            let actual = tx
                .get(&key, false)
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?;
            if hash(&actual) != expected {
                return Err(RuntimeError::RestoreUnreconciled);
            }
        }
        if result
            .insert(Discovery::subject(root), hash(&bytes))
            .is_some()
        {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    Ok(result)
}
