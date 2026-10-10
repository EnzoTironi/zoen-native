use foundationdb::{options::StreamingMode, Transaction};
use serde::{Deserialize, Serialize};

use super::{PreparedImage, Principal, Root, CHUNK_BYTES, MANIFEST_BYTES, MAX_CHUNKS};
use crate::{
    execution::{Execution, Fence, Lease},
    hash, RuntimeError,
};

const LEASE_VERSIONS: i64 = 60_000_000;
const STAGE_VERSIONS: i64 = 120_000_000;
const ORPHAN_RETENTION_VERSIONS: i64 = 60_000_000;
const RETIRED_RETENTION_VERSIONS: i64 = 60_000_000;
const MAX_RETIRED: usize = 16;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
enum StageState {
    Writing,
    Ready,
}

#[derive(Serialize, Deserialize)]
struct Stage {
    previous: Option<Root>,
    root: Root,
    holder: String,
    token: i64,
    expires: i64,
    state: StageState,
}

#[derive(Serialize, Deserialize)]
struct Retired {
    root: Root,
    collect_after: i64,
}

// Constructed only after known bounded staging commits, never deserialized.
pub(super) struct Staged {
    previous: Option<Root>,
    root: Root,
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, RuntimeError> {
    let bytes = serde_json::to_vec(value).map_err(|_| RuntimeError::InvalidBinding)?;
    if bytes.len() > 8192 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    Ok(bytes)
}

fn decode<T: for<'de> Deserialize<'de>>(value: &[u8]) -> Result<T, RuntimeError> {
    if value.len() > 8192 {
        return Err(RuntimeError::InvalidBinding);
    }
    serde_json::from_slice(value).map_err(|_| RuntimeError::InvalidBinding)
}

async fn bounded_rows(
    trx: &Transaction,
    mut range: foundationdb::RangeOption<'static>,
    max_rows: usize,
    max_value: usize,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, RuntimeError> {
    range.limit = Some(max_rows);
    range.mode = StreamingMode::WantAll;
    let mut rows = Vec::new();
    for iteration in 1..=max_rows + 1 {
        let values = trx.get_range(&range, iteration, false).await?;
        if values.is_empty() && values.more() {
            return Err(RuntimeError::Unavailable);
        }
        for value in values.iter() {
            if value.key().len() > 2048 || value.value().len() > max_value || rows.len() >= max_rows
            {
                return Err(RuntimeError::InvalidBinding);
            }
            rows.push((value.key().to_vec(), value.value().to_vec()));
        }
        match range.next_range(&values) {
            Some(next) => range = next,
            None => return Ok(rows),
        }
    }
    Err(RuntimeError::Unavailable)
}

impl Execution {
    fn native_key(&self, principal: &Principal, part: &str) -> Vec<u8> {
        self.root.pack(&(
            "native-device",
            principal.agent.as_str(),
            principal.device.as_str(),
            part,
        ))
    }

    fn native_lease(&self, principal: &Principal) -> Vec<u8> {
        self.root.pack(&(
            "device",
            principal.agent.as_str(),
            principal.device.as_str(),
            "lease",
        ))
    }

    fn native_generation(&self, root: &Root) -> foundationdb::tuple::Subspace {
        self.root.subspace(&(
            "native-image",
            root.context.principal.agent.as_str(),
            root.context.principal.device.as_str(),
            root.context.capsule.as_str(),
        ))
    }

    fn native_retired(&self, principal: &Principal) -> foundationdb::tuple::Subspace {
        self.root.subspace(&(
            "native-retired",
            principal.agent.as_str(),
            principal.device.as_str(),
        ))
    }

    fn native_references(&self, root: &Root) -> foundationdb::tuple::Subspace {
        self.root.subspace(&(
            "native-reference",
            root.context.principal.agent.as_str(),
            root.context.principal.device.as_str(),
            root.context.capsule.as_str(),
        ))
    }

    async fn native_root(
        &self,
        trx: &Transaction,
        principal: &Principal,
    ) -> Result<Option<Root>, RuntimeError> {
        trx.get(&self.native_key(principal, "root"), false)
            .await?
            .map(|value| decode(&value))
            .transpose()
    }

    pub(super) async fn acquire_native_device(
        &self,
        principal: &Principal,
    ) -> Result<Fence, RuntimeError> {
        let trx = self.transaction().await?;
        let key = self.native_lease(principal);
        let version = trx.get_read_version().await?;
        let token = if let Some(value) = trx.get(&key, false).await? {
            let prior: Lease = decode(&value)?;
            if prior.expires > version {
                return Err(RuntimeError::Denied);
            }
            prior
                .token
                .checked_add(1)
                .ok_or(RuntimeError::InvalidBinding)?
        } else {
            1
        };
        let fence = Fence {
            holder: roda_types::new_id("native-worker"),
            token,
        };
        trx.set(
            &key,
            &encode(&Lease {
                holder: fence.holder.clone(),
                token,
                expires: version
                    .checked_add(LEASE_VERSIONS)
                    .ok_or(RuntimeError::Unavailable)?,
            })?,
        );
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(fence)
    }

    pub(super) async fn release_native_device(
        &self,
        principal: &Principal,
        fence: &Fence,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        let key = self.native_lease(principal);
        let Some(value) = trx.get(&key, false).await? else {
            return Err(RuntimeError::Denied);
        };
        let mut lease: Lease = decode(&value)?;
        if lease.holder != fence.holder || lease.token != fence.token {
            return Err(RuntimeError::Denied);
        }
        lease.expires = trx.get_read_version().await?;
        trx.set(&key, &encode(&lease)?);
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }

    pub(super) async fn read_native(
        &self,
        principal: &Principal,
        fence: Option<&Fence>,
    ) -> Result<PreparedImage, RuntimeError> {
        let trx = self.transaction().await?;
        if let Some(fence) = fence {
            Self::lease(
                &trx,
                &self.native_lease(principal),
                fence,
                trx.get_read_version().await?,
            )
            .await?;
        }
        let root = self
            .native_root(&trx, principal)
            .await?
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        root.context.validate(principal, root.context.key_version)?;
        let generation = self.native_generation(&root);
        let manifest = trx
            .get(&generation.pack(&("manifest",)), false)
            .await?
            .ok_or(RuntimeError::InvalidBinding)?;
        if !(40..MANIFEST_BYTES).contains(&manifest.len()) {
            return Err(RuntimeError::InvalidBinding);
        }
        let mut chunks = Vec::new();
        for index in 0..root.context.chunks {
            let chunk = trx
                .get(&generation.pack(&("chunk", i64::from(index))), false)
                .await?
                .ok_or(RuntimeError::InvalidBinding)?;
            if !(41..=CHUNK_BYTES + 40).contains(&chunk.len()) {
                return Err(RuntimeError::InvalidBinding);
            }
            chunks.push(chunk.to_vec());
        }
        Ok(PreparedImage {
            root,
            manifest: manifest.to_vec(),
            chunks,
        })
    }

    async fn stage_scope(
        &self,
        trx: &Transaction,
        staged: &Staged,
        fence: &Fence,
    ) -> Result<Stage, RuntimeError> {
        let principal = &staged.root.context.principal;
        let version = trx.get_read_version().await?;
        Self::lease(trx, &self.native_lease(principal), fence, version).await?;
        if self.native_root(trx, principal).await? != staged.previous {
            return Err(RuntimeError::Denied);
        }
        let value = trx
            .get(&self.native_key(principal, "stage"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let stage: Stage = decode(&value)?;
        if stage.previous != staged.previous
            || stage.root != staged.root
            || stage.holder != fence.holder
            || stage.token != fence.token
            || stage.expires <= version
        {
            return Err(RuntimeError::Denied);
        }
        Ok(stage)
    }

    pub(super) async fn stage_native(
        &self,
        principal: &Principal,
        previous: Option<Root>,
        fence: &Fence,
        candidate: PreparedImage,
    ) -> Result<Staged, RuntimeError> {
        candidate
            .root
            .context
            .validate(principal, candidate.root.context.key_version)?;
        if candidate.root.context.previous != previous.as_ref().map(|r| r.manifest_digest.clone())
            || candidate.root.context.generation
                != previous
                    .as_ref()
                    .map(|r| r.context.generation)
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(RuntimeError::InvalidBinding)?
            || !(40..MANIFEST_BYTES).contains(&candidate.manifest.len())
            || hash(&candidate.manifest) != candidate.root.manifest_digest
            || candidate.chunks.len() != candidate.root.context.chunks as usize
            || candidate.chunks.len() > MAX_CHUNKS
            || candidate.chunks.iter().enumerate().any(|(index, chunk)| {
                let plain = (candidate.root.context.image_bytes as usize - index * CHUNK_BYTES)
                    .min(CHUNK_BYTES);
                chunk.len() != plain + 40
            })
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let staged = Staged {
            previous,
            root: candidate.root,
        };
        let trx = self.transaction().await?;
        let version = trx.get_read_version().await?;
        Self::lease(&trx, &self.native_lease(principal), fence, version).await?;
        if self.native_root(&trx, principal).await? != staged.previous
            || trx
                .get(&self.native_key(principal, "stage"), false)
                .await?
                .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        let generation = self.native_generation(&staged.root);
        let (begin, end) = generation.range();
        let mut range = foundationdb::RangeOption::from((begin.as_slice(), end.as_slice()));
        range.limit = Some(1);
        if !trx.get_range(&range, 1, false).await?.is_empty() {
            return Err(RuntimeError::InvalidBinding);
        }
        trx.set(
            &self.native_key(principal, "stage"),
            &encode(&Stage {
                previous: staged.previous.clone(),
                root: staged.root.clone(),
                holder: fence.holder.clone(),
                token: fence.token,
                expires: version
                    .checked_add(STAGE_VERSIONS)
                    .ok_or(RuntimeError::Unavailable)?,
                state: StageState::Writing,
            })?,
        );
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        for (index, chunk) in candidate.chunks.iter().enumerate() {
            let trx = self.transaction().await?;
            self.stage_scope(&trx, &staged, fence).await?;
            let key = generation.pack(&("chunk", index as i64));
            if let Some(prior) = trx.get(&key, false).await? {
                if prior.as_ref() != chunk.as_slice() {
                    return Err(RuntimeError::InvalidBinding);
                }
            } else {
                trx.set(&key, chunk);
            }
            trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        }
        let trx = self.transaction().await?;
        let mut stage = self.stage_scope(&trx, &staged, fence).await?;
        for (index, expected) in candidate.chunks.iter().enumerate() {
            let value = trx
                .get(&generation.pack(&("chunk", index as i64)), false)
                .await?
                .ok_or(RuntimeError::InvalidBinding)?;
            if value.as_ref() != expected.as_slice() {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        let manifest_key = generation.pack(&("manifest",));
        if trx.get(&manifest_key, false).await?.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        trx.set(&manifest_key, &candidate.manifest);
        stage.state = StageState::Ready;
        trx.set(&self.native_key(principal, "stage"), &encode(&stage)?);
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(staged)
    }

    pub(super) async fn activate_native(
        &self,
        staged: Staged,
        device_fence: &Fence,
    ) -> Result<Root, RuntimeError> {
        let trx = self.native_activation(&staged, device_fence).await?;
        // No automatic retry may recover an activation as a live workspace or
        // capability after an uncertain commit.
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(staged.root)
    }

    async fn native_activation(
        &self,
        staged: &Staged,
        device_fence: &Fence,
    ) -> Result<Transaction, RuntimeError> {
        let trx = self.transaction().await?;
        let stage = self.stage_scope(&trx, staged, device_fence).await?;
        if stage.state != StageState::Ready {
            return Err(RuntimeError::Denied);
        }
        let manifest = trx
            .get(
                &self.native_generation(&staged.root).pack(&("manifest",)),
                false,
            )
            .await?
            .ok_or(RuntimeError::InvalidBinding)?;
        if hash(&manifest) != staged.root.manifest_digest {
            return Err(RuntimeError::InvalidBinding);
        }
        let principal = &staged.root.context.principal;
        if let Some(previous) = &staged.previous {
            let retained = bounded_rows(
                &trx,
                foundationdb::RangeOption::from(self.native_retired(principal).range()),
                MAX_RETIRED,
                8192,
            )
            .await?;
            if retained.len() >= MAX_RETIRED {
                return Err(RuntimeError::EvidenceTooLarge);
            }
            trx.set(
                &self
                    .native_retired(principal)
                    .pack(&(previous.context.capsule.as_str(),)),
                &encode(&Retired {
                    root: previous.clone(),
                    collect_after: trx
                        .get_read_version()
                        .await?
                        .checked_add(RETIRED_RETENTION_VERSIONS)
                        .ok_or(RuntimeError::Unavailable)?,
                })?,
            );
        }
        trx.set(&self.native_key(principal, "root"), &encode(&staged.root)?);
        trx.clear(&self.native_key(principal, "stage"));
        // GC reads these same root/pin keys and conflicts with activation.
        Ok(trx)
    }

    pub(super) async fn collect_native_stage(
        &self,
        principal: &Principal,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        let key = self.native_key(principal, "stage");
        let Some(value) = trx.get(&key, false).await? else {
            return Ok(());
        };
        let stage: Stage = decode(&value)?;
        stage
            .root
            .context
            .validate(principal, stage.root.context.key_version)?;
        let version = trx.get_read_version().await?;
        let collect_after = stage
            .expires
            .checked_add(ORPHAN_RETENTION_VERSIONS)
            .ok_or(RuntimeError::InvalidBinding)?;
        if version < collect_after {
            return Ok(());
        }
        if self
            .native_root(&trx, principal)
            .await?
            .is_some_and(|r| r.context.capsule == stage.root.context.capsule)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let (begin, end) = self.native_generation(&stage.root).range();
        trx.clear_range(&begin, &end);
        trx.clear(&key);
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }

    pub(super) async fn collect_native_retired(
        &self,
        principal: &Principal,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        let version = trx.get_read_version().await?;
        let active = self.native_root(&trx, principal).await?;
        let stage: Option<Stage> = trx
            .get(&self.native_key(principal, "stage"), false)
            .await?
            .map(|value| decode(&value))
            .transpose()?;
        let retired_rows = bounded_rows(
            &trx,
            foundationdb::RangeOption::from(self.native_retired(principal).range()),
            MAX_RETIRED,
            8192,
        )
        .await?;
        let mut changed = false;
        for (key, bytes) in retired_rows {
            let retired: Retired = decode(&bytes)?;
            retired
                .root
                .context
                .validate(principal, retired.root.context.key_version)?;
            let (capsule,): (String,) = self
                .native_retired(principal)
                .unpack(&key)
                .map_err(|_| RuntimeError::InvalidBinding)?;
            if capsule != retired.root.context.capsule {
                return Err(RuntimeError::InvalidBinding);
            }
            if retired.collect_after > version
                || active
                    .as_ref()
                    .is_some_and(|r| r.context.capsule == capsule)
                || stage.as_ref().is_some_and(|s| {
                    s.root.context.capsule == capsule
                        || s.previous
                            .as_ref()
                            .is_some_and(|r| r.context.capsule == capsule)
                })
            {
                continue;
            }
            let mut references =
                foundationdb::RangeOption::from(self.native_references(&retired.root).range());
            references.limit = Some(1);
            if !trx.get_range(&references, 1, false).await?.is_empty() {
                continue;
            }
            let (begin, end) = self.native_generation(&retired.root).range();
            trx.clear_range(&begin, &end);
            trx.clear(&key);
            changed = true;
        }
        if changed {
            // Root, stage and reference range reads conflict with any concurrent
            // activation or retained-reference insertion. Pinned evidence stays.
            trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) async fn test_cuts(
    execution: &Execution,
    custody: &super::NativeCustody,
    principal: &Principal,
    image: &roda_ffi::runtime_core::NativeImage,
) {
    let current = execution.read_native(principal, None).await.unwrap().root;
    let trx = execution.transaction().await.unwrap();
    let mut run_range = foundationdb::RangeOption::from(execution.root.subspace(&("run",)).range());
    run_range.limit = Some(1);
    assert!(
        trx.get_range(&run_range, 1, false)
            .await
            .unwrap()
            .is_empty(),
        "device maintenance has no fabricated run"
    );
    drop(trx);
    let fence = execution.acquire_native_device(principal).await.unwrap();
    assert!(matches!(
        execution.acquire_native_device(principal).await,
        Err(RuntimeError::Denied)
    ));
    let staged = execution
        .stage_native(
            principal,
            Some(current.clone()),
            &fence,
            custody
                .seal_image(principal, Some(&current), image)
                .unwrap(),
        )
        .await
        .unwrap();
    let generation = execution.native_generation(&staged.root);
    execution.collect_native_stage(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(
        trx.get(&generation.pack(&("manifest",)), false)
            .await
            .unwrap()
            .is_some(),
        "live pin protects staging"
    );
    assert_eq!(
        execution.native_root(&trx, principal).await.unwrap(),
        Some(current.clone())
    );
    let values = bounded_rows(
        &trx,
        foundationdb::RangeOption::from(generation.range()),
        MAX_CHUNKS + 1,
        CHUNK_BYTES + 40,
    )
    .await
    .unwrap();
    assert_eq!(values.len(), staged.root.context.chunks as usize + 1);
    for (_, bytes) in values {
        assert!(!bytes.windows(15).any(|w| w == b"SQLite format 3"));
        assert!(!bytes.windows(32).any(|w| w == [73; 32]));
    }
    drop(trx);
    let pending = execution.native_activation(&staged, &fence).await.unwrap();
    let concurrent = execution.transaction().await.unwrap();
    concurrent.set(
        &execution.native_key(principal, "root"),
        &encode(&current).unwrap(),
    );
    concurrent.commit().await.unwrap();
    assert!(
        pending.commit().await.is_err(),
        "a concurrent root write conflicts with activation"
    );
    assert_eq!(
        execution.read_native(principal, None).await.unwrap().root,
        current
    );
    execution
        .release_native_device(principal, &fence)
        .await
        .unwrap();
    let takeover = execution.acquire_native_device(principal).await.unwrap();
    assert!(takeover.token > fence.token);
    assert!(matches!(
        execution.native_activation(&staged, &fence).await,
        Err(RuntimeError::Denied)
    ));
    assert!(
        matches!(
            execution.native_activation(&staged, &takeover).await,
            Err(RuntimeError::Denied)
        ),
        "new holder cannot adopt an old holder's staged workspace"
    );
    assert_eq!(
        execution
            .release_native_device(principal, &fence)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    let trx = execution.transaction().await.unwrap();
    let stage_key = execution.native_key(principal, "stage");
    let mut expired: Stage = decode(&trx.get(&stage_key, false).await.unwrap().unwrap()).unwrap();
    // Explicit version-clock fixture cut; no elapsed retention or wire-level
    // commit-unknown fault is claimed by this setup.
    expired.expires = trx.get_read_version().await.unwrap() - ORPHAN_RETENTION_VERSIONS - 1;
    trx.set(&stage_key, &encode(&expired).unwrap());
    trx.commit().await.unwrap();
    execution.collect_native_stage(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(trx
        .get(&generation.pack(&("manifest",)), false)
        .await
        .unwrap()
        .is_none());
    assert!(trx.get(&stage_key, false).await.unwrap().is_none());
    assert_eq!(
        execution.native_root(&trx, principal).await.unwrap(),
        Some(current.clone())
    );
    assert!(
        trx.get(
            &execution.native_generation(&current).pack(&("manifest",)),
            false
        )
        .await
        .unwrap()
        .is_some(),
        "GC preserves active image"
    );
    drop(trx);
    let next = execution
        .stage_native(
            principal,
            Some(current.clone()),
            &takeover,
            custody
                .seal_image(principal, Some(&current), image)
                .unwrap(),
        )
        .await
        .unwrap();
    let expected = next.root.clone();
    let activated = execution.activate_native(next, &takeover).await.unwrap();
    // Suppress a known committed activation reply and reload the durable root;
    // there is no recreated live dispatch capability.
    drop(activated);
    let restored = execution
        .read_native(principal, Some(&takeover))
        .await
        .unwrap();
    assert_eq!(restored.root, expected);
    assert_eq!(
        custody.open_image(principal, restored).unwrap().as_bytes(),
        image.as_bytes()
    );
    execution.collect_native_retired(principal).await.unwrap();
    let previous_key = execution
        .native_retired(principal)
        .pack(&(current.context.capsule.as_str(),));
    let trx = execution.transaction().await.unwrap();
    let mut retired: Retired =
        decode(&trx.get(&previous_key, false).await.unwrap().unwrap()).unwrap();
    assert!(
        trx.get(
            &execution.native_generation(&current).pack(&("manifest",)),
            false
        )
        .await
        .unwrap()
        .is_some(),
        "retention protects the previous image"
    );
    retired.collect_after = trx.get_read_version().await.unwrap() - 1;
    trx.set(&previous_key, &encode(&retired).unwrap());
    let reference = execution
        .native_references(&current)
        .pack(&("pending-original-attempt",));
    // Explicit retained-reference fixture, not a production run or admission.
    trx.set(&reference, b"1");
    trx.commit().await.unwrap();
    execution.collect_native_retired(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(
        trx.get(&previous_key, false).await.unwrap().is_some(),
        "a retained reference prevents collection"
    );
    trx.clear(&reference);
    trx.commit().await.unwrap();
    execution.collect_native_retired(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(trx.get(&previous_key, false).await.unwrap().is_none());
    assert!(trx
        .get(
            &execution.native_generation(&current).pack(&("manifest",)),
            false
        )
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        execution.native_root(&trx, principal).await.unwrap(),
        Some(expected.clone())
    );
    let active_retirement_key = execution
        .native_retired(principal)
        .pack(&(expected.context.capsule.as_str(),));
    let mut corrupt_metadata = expected.clone();
    corrupt_metadata.manifest_digest = hash(b"corrupt retirement metadata");
    trx.set(
        &active_retirement_key,
        &encode(&Retired {
            root: corrupt_metadata,
            collect_after: 0,
        })
        .unwrap(),
    );
    trx.commit().await.unwrap();
    execution.collect_native_retired(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(
        trx.get(
            &execution.native_generation(&expected).pack(&("manifest",)),
            false
        )
        .await
        .unwrap()
        .is_some(),
        "physical active capsule remains protected even with a mismatching retirement digest"
    );
    trx.clear(&active_retirement_key);
    trx.commit().await.unwrap();
    execution
        .release_native_device(principal, &takeover)
        .await
        .unwrap();
}
