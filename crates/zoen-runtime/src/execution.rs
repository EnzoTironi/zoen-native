use crate::{custody::Custody, finance::FreshClaim, Binding, RuntimeError, VerifiedStep};
use foundationdb::{
    options::TransactionOption, tuple::Subspace, Database, RangeOption, Transaction,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub(crate) struct Execution {
    pub(super) db: Arc<Database>,
    pub(super) root: Subspace,
    deployment: String,
}
#[derive(Clone)]
pub(crate) struct Fence {
    pub holder: String,
    pub token: i64,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Lease {
    pub(super) holder: String,
    pub(super) token: i64,
    pub(super) expires: i64,
}
#[derive(Serialize, Deserialize)]
struct CoreCapsule {
    request: zoen_models::ModelRequest,
    frontier: String,
    policy_digest: String,
}
pub(crate) struct FreshAdmission {
    attempt: String,
    nonce: String,
}
impl FreshAdmission {
    pub fn consume(self) -> (String, String) {
        (self.attempt, self.nonce)
    }
}
pub(crate) struct PreDispatchClosure {
    binding: Binding,
    digest: String,
}
impl PreDispatchClosure {
    pub fn consume(self) -> (Binding, String) {
        (self.binding, self.digest)
    }
}

impl Execution {
    pub fn new(db: Arc<Database>, root: Subspace, deployment: String) -> Self {
        Self {
            db,
            root,
            deployment,
        }
    }
    fn bounded_transaction(db: &Database) -> Result<Transaction, RuntimeError> {
        let trx = db.create_trx()?;
        trx.set_option(TransactionOption::Timeout(2000))?;
        // No retry helper or on_error: one commit result may mint a live capability.
        Ok(trx)
    }
    pub async fn verify_deployment(
        db: &Database,
        root: &Subspace,
        expected: &str,
    ) -> Result<(), RuntimeError> {
        let trx = Self::bounded_transaction(db)?;
        Self::check_deployment(&trx, root, expected).await
    }
    pub async fn create_deployment(
        db: &Database,
        root: &Subspace,
        witness: &str,
    ) -> Result<(), RuntimeError> {
        let trx = Self::bounded_transaction(db)?;
        let (begin, end) = root.range();
        let mut range = RangeOption::from((begin.as_slice(), end.as_slice()));
        range.limit = Some(1);
        if !trx.get_range(&range, 1, false).await?.is_empty() {
            return Err(RuntimeError::DeploymentMismatch);
        }
        trx.set(&root.pack(&("deployment",)), witness.as_bytes());
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
    async fn check_deployment(
        trx: &Transaction,
        root: &Subspace,
        expected: &str,
    ) -> Result<(), RuntimeError> {
        let marker = trx.get(&root.pack(&("deployment",)), false).await?;
        if marker.as_deref() != Some(expected.as_bytes()) {
            return Err(RuntimeError::DeploymentMismatch);
        }
        Ok(())
    }
    pub(super) async fn transaction(&self) -> Result<Transaction, RuntimeError> {
        let trx = Self::bounded_transaction(&self.db)?;
        // Guard each operation and its commit against an unpaired store or a
        // changed marker, including already-open runtimes after reconfiguration.
        Self::check_deployment(&trx, &self.root, &self.deployment).await?;
        Ok(trx)
    }
    fn run(&self, run: &str, part: &str) -> Vec<u8> {
        self.root.pack(&("run", run, part))
    }
    fn device(&self, context: &zoen_models::AttemptContext) -> Vec<u8> {
        self.root.pack(&(
            "device",
            context.agent.as_str(),
            context.device.as_str(),
            "lease",
        ))
    }
    fn attempt(&self, attempt: &str, part: &str) -> Vec<u8> {
        self.root.pack(&("attempt", attempt, part))
    }
    pub(super) async fn lease(
        trx: &Transaction,
        key: &[u8],
        fence: &Fence,
        version: i64,
    ) -> Result<(), RuntimeError> {
        let value = trx.get(key, false).await?.ok_or(RuntimeError::Denied)?;
        let lease: Lease =
            serde_json::from_slice(&value).map_err(|_| RuntimeError::InvalidBinding)?;
        if lease.holder != fence.holder || lease.token != fence.token || lease.expires <= version {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }
    async fn scope(
        &self,
        trx: &Transaction,
        step: &VerifiedStep,
        custody: &Custody,
        cancelling: bool,
    ) -> Result<(), RuntimeError> {
        let ctx = &step.request.context;
        let version = trx.get_read_version().await?;
        Self::lease(
            trx,
            &self.run(&ctx.run_id, "lease"),
            &step.run_fence,
            version,
        )
        .await?;
        Self::lease(trx, &self.device(ctx), &step.device_fence, version).await?;
        if !cancelling
            && trx
                .get(&self.run(&ctx.run_id, "cancel"), false)
                .await?
                .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        let sealed = trx
            .get(&self.run(&ctx.run_id, "core"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let capsule: CoreCapsule = custody.open(&format!("core/{}", ctx.run_id), None, &sealed)?;
        let frontier = trx
            .get(&self.root.pack(&("frontier", ctx.space.as_str())), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        if capsule.request != step.request
            || capsule.frontier != step.frontier
            || capsule.policy_digest != step.policy_digest
            || frontier.as_ref() != step.frontier.as_bytes()
        {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }
    pub async fn prepare(
        &self,
        step: &VerifiedStep,
        binding: &Binding,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        self.scope(&trx, step, custody, false).await?;
        let attempt = &binding.context.attempt_id;
        let key = self.attempt(attempt, "prepared");
        if let Some(old) = trx.get(&key, false).await? {
            let prior: Binding = custody.open(&format!("prepared/{attempt}"), None, &old)?;
            if prior != *binding {
                return Err(RuntimeError::InvalidBinding);
            }
            return Ok(());
        }
        if trx
            .get(&self.attempt(attempt, "closed"), false)
            .await?
            .is_some()
            || trx
                .get(&self.attempt(attempt, "admitted"), false)
                .await?
                .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        let (_, sealed) = custody.seal(&format!("prepared/{attempt}"), binding)?;
        trx.set(&key, &sealed);
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
    pub async fn admit_once(
        &self,
        step: &VerifiedStep,
        claim: FreshClaim,
        binding: &Binding,
        custody: &Custody,
    ) -> Result<FreshAdmission, RuntimeError> {
        let (attempt, nonce, digest) = claim.consume();
        if attempt != binding.context.attempt_id || digest != binding.digest()? {
            return Err(RuntimeError::InvalidBinding);
        }
        let trx = self.transaction().await?;
        self.scope(&trx, step, custody, false).await?;
        let prepared = trx
            .get(&self.attempt(&attempt, "prepared"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let prior: Binding = custody.open(&format!("prepared/{attempt}"), None, &prepared)?;
        if prior != *binding {
            return Err(RuntimeError::InvalidBinding);
        }
        if trx
            .get(&self.attempt(&attempt, "closed"), false)
            .await?
            .is_some()
            || trx
                .get(&self.attempt(&attempt, "admitted"), false)
                .await?
                .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        // The unique SQL nonce is evidence of the consumed claim, never a read API.
        trx.set(&self.attempt(&attempt, "admitted"), nonce.as_bytes());
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(FreshAdmission { attempt, nonce })
    }
    pub async fn close_before_dispatch(
        &self,
        step: &VerifiedStep,
        binding: &Binding,
        custody: &Custody,
        runtime: &crate::RuntimeAuthority,
    ) -> Result<Option<PreDispatchClosure>, RuntimeError> {
        runtime.continuity()?;
        let trx = self.transaction().await?;
        self.scope(&trx, step, custody, true).await?;
        let attempt = &binding.context.attempt_id;
        let prepared = trx
            .get(&self.attempt(attempt, "prepared"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let prior: Binding = custody.open(&format!("prepared/{attempt}"), None, &prepared)?;
        if prior != *binding {
            return Err(RuntimeError::InvalidBinding);
        }
        trx.set(&self.run(&binding.context.run_id, "cancel"), b"1");
        if trx
            .get(&self.attempt(attempt, "admitted"), false)
            .await?
            .is_some()
        {
            trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            return Ok(None);
        }
        let digest = binding.digest()?;
        let key = self.attempt(attempt, "closed");
        if let Some(old) = trx.get(&key, false).await? {
            if old.as_ref() != digest.as_bytes() {
                return Err(RuntimeError::InvalidBinding);
            }
            trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            runtime.continuity()?;
            return Ok(Some(PreDispatchClosure {
                binding: binding.clone(),
                digest,
            }));
        }
        trx.set(&key, digest.as_bytes());
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        runtime.continuity()?;
        Ok(Some(PreDispatchClosure {
            binding: binding.clone(),
            digest,
        }))
    }
    pub async fn prepared_binding(
        &self,
        attempt: &str,
        custody: &Custody,
    ) -> Result<Binding, RuntimeError> {
        let trx = self.transaction().await?;
        let sealed = trx
            .get(&self.attempt(attempt, "prepared"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        custody.open(&format!("prepared/{attempt}"), None, &sealed)
    }
    pub async fn retain_evidence(
        &self,
        attempt: &str,
        digest: &str,
        sealed: &[u8],
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        if trx
            .get(&self.attempt(attempt, "admitted"), false)
            .await?
            .is_none()
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let key = self.root.pack(&("evidence", attempt, digest));
        if trx.get(&key, false).await?.is_none() {
            trx.set(&key, sealed);
            trx.set(&self.root.pack(&("transfer", attempt, digest)), sealed);
        }
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
    pub async fn pending_evidence(
        &self,
        limit: u16,
    ) -> Result<Vec<(String, String, Vec<u8>)>, RuntimeError> {
        let subspace = self.root.subspace(&("transfer",));
        let mut range = RangeOption::from(subspace.range());
        range.limit = Some(usize::from(limit));
        let trx = self.transaction().await?;
        let values = trx.get_range(&range, 1, true).await?;
        values
            .iter()
            .map(|v| {
                let (attempt, digest): (String, String) = subspace
                    .unpack(v.key())
                    .map_err(|_| RuntimeError::InvalidBinding)?;
                Ok((attempt, digest, v.value().to_vec()))
            })
            .collect()
    }
    pub async fn ack_evidence(&self, attempt: &str, digest: &str) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        trx.clear(&self.root.pack(&("transfer", attempt, digest)));
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
    pub async fn financial_reference(
        &self,
        attempt: &str,
        kind: &str,
        digest: &str,
        sealed: Option<&[u8]>,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        let reference = self.root.pack(&("financial", attempt, kind, digest));
        if trx.get(&reference, false).await?.is_none() {
            trx.set(&reference, b"1");
            if kind == "evidence" {
                let sealed = sealed.ok_or(RuntimeError::InvalidBinding)?;
                let key = self.root.pack(&("evidence", attempt, digest));
                if trx.get(&key, false).await?.is_none() {
                    trx.set(&key, sealed);
                }
            }
        }
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
    // Fixture core projections are compiled only in tests. They do not implement
    // MLS sender, ordered grants, approvals or certified device-state import.
    #[cfg(test)]
    pub async fn seed(&self, step: &VerifiedStep, custody: &Custody) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        let expires = trx
            .get_read_version()
            .await?
            .checked_add(60_000_000)
            .ok_or(RuntimeError::Unavailable)?;
        let context = &step.request.context;
        for (key, fence) in [
            (self.run(&context.run_id, "lease"), &step.run_fence),
            (self.device(context), &step.device_fence),
        ] {
            trx.set(
                &key,
                &serde_json::to_vec(&Lease {
                    holder: fence.holder.clone(),
                    token: fence.token,
                    expires,
                })
                .unwrap(),
            );
        }
        let capsule = CoreCapsule {
            request: step.request.clone(),
            frontier: step.frontier.clone(),
            policy_digest: step.policy_digest.clone(),
        };
        let (_, sealed) = custody.seal(&format!("core/{}", context.run_id), &capsule)?;
        trx.set(&self.run(&context.run_id, "core"), &sealed);
        trx.set(
            &self.root.pack(&("frontier", context.space.as_str())),
            step.frontier.as_bytes(),
        );
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
    #[cfg(test)]
    pub async fn has_admission(&self, attempt: &str) -> bool {
        self.transaction()
            .await
            .unwrap()
            .get(&self.attempt(attempt, "admitted"), false)
            .await
            .unwrap()
            .is_some()
    }
    #[cfg(test)]
    pub async fn take_device(&self, step: &VerifiedStep) {
        let trx = self.transaction().await.unwrap();
        let expires = trx.get_read_version().await.unwrap() + 60_000_000;
        trx.set(
            &self.device(&step.request.context),
            &serde_json::to_vec(&Lease {
                holder: "new-owner".into(),
                token: step.device_fence.token + 1,
                expires,
            })
            .unwrap(),
        );
        trx.commit().await.unwrap();
    }
}
