//! Postgres finance and fenced FDB execution. Paid production work stays closed
//! until the actual certified core/MLS retained-step loader is integrated.
mod custody;
mod execution;
mod finance;
mod native;
mod pricing;
pub use pricing::PriceProfile;
pub use roda_types::owner_budget::{OwnerPeriodPolicy, SignedOwnerPolicy};

use async_trait::async_trait;
use foundationdb::{tuple::Subspace, Database};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use zoen_models::{
    Admission, AdmissionError, AttemptContext, DispatchAuthority, DispatchRequest, GatewayConfig,
    ModelAttemptResult, ModelGateway, ModelRequest,
};

pub struct RuntimeConfig {
    pub postgres_url: String,
    pub fdb_cluster_file: Option<String>,
    pub namespace: String,
    /// Actual relay cell on the shared FDB cluster, pinned at first open.
    pub relay_cell: Option<String>,
    pub price: PriceProfile,
    pub gateway: GatewayConfig,
    pub evidence_key: [u8; 32],
}
impl std::fmt::Debug for RuntimeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RuntimeConfig { redacted }")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeError {
    #[error("invalid signed owner policy")]
    InvalidPolicy,
    #[error("unsupported finite price profile")]
    InvalidProfile,
    #[error("runtime deployment does not match the financial database")]
    DeploymentMismatch,
    #[error("conflicting immutable binding")]
    InvalidBinding,
    #[error("runtime authority denied")]
    Denied,
    #[error("owner period budget exhausted")]
    OverBudget,
    #[error("actual core and MLS authority unavailable")]
    CoreAuthorityUnavailable,
    #[error("cross-store restore continuity unavailable")]
    RestoreUnreconciled,
    #[error("runtime store unavailable or commit uncertain")]
    Unavailable,
    #[error("bounded evidence cannot be retained")]
    EvidenceTooLarge,
}
impl From<sqlx::Error> for RuntimeError {
    fn from(error: sqlx::Error) -> Self {
        #[cfg(test)]
        eprintln!(
            "runtime SQL failure category: {}",
            match &error {
                sqlx::Error::Database(db) => db
                    .code()
                    .map(|code| code.into_owned())
                    .unwrap_or_else(|| "database".into()),
                sqlx::Error::PoolTimedOut => "pool-timeout".into(),
                sqlx::Error::PoolClosed => "pool-closed".into(),
                _ => "other".into(),
            }
        );
        let _ = error;
        Self::Unavailable
    }
}
impl From<foundationdb::FdbError> for RuntimeError {
    fn from(_: foundationdb::FdbError) -> Self {
        Self::Unavailable
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinancialState {
    Reserved,
    Claimed,
    Settled { units: i64 },
    Released,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeriodBalance {
    pub held_units: i64,
    pub spent_units: i64,
}
#[derive(Debug, Default)]
pub struct ReconcileReport {
    pub inspected: u16,
    pub transferred: u16,
    pub parked: u16,
}
#[derive(Debug)]
pub struct RetainedModel {
    pub financial: FinancialState,
}

/// Structural inspection of a fully authenticated native capsule. This does
/// not certify a current relay head or grant permission to dispatch a model.
#[derive(Debug, PartialEq, Eq)]
pub struct NativeInspection {
    pub generation: u64,
    pub image_bytes: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyRun {
    pub run: String,
    pub attempt: String,
    pub wake: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReplySync {
    pub native: NativeInspection,
    pub caught_up: bool,
    pub scanned: u16,
    pub discovery_complete: bool,
    pub runs: Vec<ReplyRun>,
}

pub struct RuntimeAuthority {
    finance: finance::Finance,
    execution: execution::Execution,
    custody: custody::Custody,
    // No production key or credential constructor exists yet. Test provisioning
    // exercises the storage protocol without claiming a deployed managed vault.
    native: Option<native::NativeCustody>,
    gateway: ModelGateway,
    price: PriceProfile,
    // No production setter: an incarnation UUID does not prove absence after restore.
    continuity: Arc<AtomicBool>,
    #[cfg(test)]
    fault: std::sync::atomic::AtomicU8,
    #[cfg(test)]
    native_cut_entered: tokio::sync::Notify,
    #[cfg(test)]
    native_cut_resume: tokio::sync::Notify,
}

impl RuntimeAuthority {
    /// The host owns FoundationDB's process-wide network lifetime, as for FdbLog.
    /// This library never boots a second client network.
    pub async fn open(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        config.price.validate()?;
        if !id(&config.namespace) {
            return Err(RuntimeError::InvalidBinding);
        }
        let db = Arc::new(Database::new(config.fdb_cluster_file.as_deref())?);
        let gateway =
            ModelGateway::new(config.gateway).map_err(|_| RuntimeError::InvalidProfile)?;
        let finance = finance::Finance::connect(&config.postgres_url).await?;
        let root = Subspace::from_bytes(foundationdb::tuple::pack(&(
            "zoen-runtime-v1",
            config.namespace.as_str(),
        )));
        let witness = finance
            .bind_deployment(&config.namespace, &hash(&config.evidence_key), &db, &root)
            .await?;
        let source = config
            .relay_cell
            .as_deref()
            .map(zoen_relay::log::fdb::RuntimeSource::configured)
            .transpose()
            .map_err(|_| RuntimeError::InvalidBinding)?;
        execution::Execution::bind_source(&db, &root, &witness, source.as_ref()).await?;
        Ok(Self {
            finance,
            execution: execution::Execution::new(db, root, witness, source),
            custody: custody::Custody::new(config.evidence_key, config.namespace),
            native: None,
            gateway,
            price: config.price,
            continuity: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            fault: std::sync::atomic::AtomicU8::new(0),
            #[cfg(test)]
            native_cut_entered: tokio::sync::Notify::new(),
            #[cfg(test)]
            native_cut_resume: tokio::sync::Notify::new(),
        })
    }
    pub async fn install_policy(&self, signed: SignedOwnerPolicy) -> Result<String, RuntimeError> {
        self.finance.install_policy(signed).await
    }
    pub async fn balance(
        &self,
        owner: &str,
        year: i32,
        month: u8,
    ) -> Result<PeriodBalance, RuntimeError> {
        self.finance.balance(owner, year, month).await
    }
    pub async fn inspect_attempt(
        &self,
        attempt: &str,
    ) -> Result<Option<FinancialState>, RuntimeError> {
        if !id(attempt) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.finance.inspect(attempt).await
    }
    pub async fn run_model(&self, run: &str) -> Result<RetainedModel, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        let step = self.verified_step(run).await?;
        let (_, financial) = self.complete_verified(&step).await?;
        Ok(RetainedModel { financial })
    }
    pub async fn inspect_native_device(
        &self,
        agent: &str,
        device: &str,
    ) -> Result<NativeInspection, RuntimeError> {
        if !id(agent) || !id(device) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .inspect(self, agent, device)
            .await
    }

    /// Repack one already provisioned device under its actual device fence.
    /// No run, trigger, external effect or financial permission is created.
    pub async fn repack_native_device(
        &self,
        agent: &str,
        device: &str,
    ) -> Result<NativeInspection, RuntimeError> {
        if !id(agent) || !id(device) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .repack(self, agent, device)
            .await
    }

    /// Locators only. The host cannot supply a frontier, trigger, request,
    /// native image, grant, credential or verified execution step.
    pub async fn sync_reply_runs(
        &self,
        agent: &str,
        device: &str,
        space: &str,
    ) -> Result<ReplySync, RuntimeError> {
        if !id(agent) || !id(device) || !id(space) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .sync_replies(self, agent, device, space)
            .await
    }

    /// Exercises the private retained loader and returns inert identifiers.
    /// This does not admit a paid model call or export native context.
    pub async fn inspect_reply_run(&self, run: &str) -> Result<ReplyRun, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .inspect_reply(self, run)
            .await
    }

    /// Durable scheduling locators, not execution capabilities. A lost wake
    /// reply cannot create another run or another paid attempt.
    pub async fn pending_reply_runs(&self, limit: u16) -> Result<Vec<ReplyRun>, RuntimeError> {
        self.execution
            .pending_reply_runs(limit, &self.custody)
            .await
    }
    async fn verified_step(&self, _run: &str) -> Result<VerifiedStep, RuntimeError> {
        // No injected verifier, supplied head, arbitrary request or enrollment-only
        // grant can construct a verified retained step through this public API.
        Err(RuntimeError::CoreAuthorityUnavailable)
    }
    pub async fn cancel_model(&self, run: &str) -> Result<FinancialState, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        let step = self.verified_step(run).await?;
        self.continuity()?;
        let binding = self
            .execution
            .prepared_binding(&step.request.context.attempt_id, &self.custody)
            .await?;
        match self
            .execution
            .close_before_dispatch(&step, &binding, &self.custody, self)
            .await?
        {
            Some(proof) => self.finance.release(proof, self).await,
            None => self
                .finance
                .inspect(&binding.context.attempt_id)
                .await?
                .ok_or(RuntimeError::InvalidBinding),
        }
    }
    pub async fn reconcile(&self, limit: u16) -> Result<ReconcileReport, RuntimeError> {
        if !(1..=64).contains(&limit) {
            return Err(RuntimeError::InvalidBinding);
        }
        let mut report = ReconcileReport::default();
        for (attempt, digest, sealed) in self.execution.pending_evidence(limit).await? {
            report.inspected += 1;
            let capture: Capture =
                self.custody
                    .open(&format!("evidence/{attempt}"), Some(&digest), &sealed)?;
            if self
                .finance
                .capture(&capture, &digest, &sealed)
                .await
                .is_ok()
            {
                self.execution.ack_evidence(&attempt, &digest).await?;
                report.transferred += 1;
            } else {
                report.parked += 1;
            }
        }
        let remaining = limit.saturating_sub(report.inspected);
        for (attempt, kind, digest, sealed) in self.finance.pending_outbox(remaining).await? {
            report.inspected += 1;
            if self
                .execution
                .financial_reference(&attempt, &kind, &digest, sealed.as_deref())
                .await
                .is_ok()
            {
                self.finance.ack_outbox(&attempt, &kind, &digest).await?;
                report.transferred += 1;
            } else {
                report.parked += 1;
            }
        }
        Ok(report)
    }
    fn continuity(&self) -> Result<(), RuntimeError> {
        if self.continuity.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(RuntimeError::RestoreUnreconciled)
        }
    }
    async fn complete_verified(
        &self,
        step: &VerifiedStep,
    ) -> Result<(ModelAttemptResult, FinancialState), RuntimeError> {
        self.continuity()?;
        let session = Session {
            runtime: self,
            step,
            admitted: Mutex::new(None),
        };
        let result = self
            .gateway
            .complete(step.request.clone(), &session)
            .await
            .map_err(|e| match e {
                zoen_models::GatewayError::Admission(AdmissionError::Denied) => {
                    RuntimeError::Denied
                }
                _ => RuntimeError::Unavailable,
            })?;
        let binding = session
            .admitted
            .lock()
            .map_err(|_| RuntimeError::Unavailable)?
            .take()
            .ok_or(RuntimeError::InvalidBinding)?;
        let capture = Capture {
            binding,
            result: result.clone(),
        };
        let (digest, sealed) = self.custody.seal(
            &format!("evidence/{}", capture.binding.context.attempt_id),
            &capture,
        )?;
        // Either durable store can retain incurred evidence. It grants no progress.
        let sql = self.finance.capture(&capture, &digest, &sealed).await;
        #[cfg(test)]
        let sql = if self.fault.load(Ordering::SeqCst) == 5 {
            Err(RuntimeError::Unavailable)
        } else {
            sql
        };
        let fdb = self
            .execution
            .retain_evidence(&capture.binding.context.attempt_id, &digest, &sealed)
            .await;
        if sql.is_err() && fdb.is_err() {
            return Err(RuntimeError::Unavailable);
        }
        Ok((result, sql.unwrap_or(FinancialState::Claimed)))
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Binding {
    context: AttemptContext,
    device_cert: String,
    frontier: String,
    period_start: i64,
    policy_digest: String,
    request_digest: String,
    requested_output_tokens: u64,
    hold_units: i64,
    price: PriceProfile,
}
impl Binding {
    fn digest(&self) -> Result<String, RuntimeError> {
        Ok(hash(
            &serde_json::to_vec(self).map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
}
// Private and deliberately lacks Deserialize/general-purpose constructor.
// Only actual core extraction may create one in production; fixtures live in cfg(test).
struct VerifiedStep {
    request: ModelRequest,
    device_cert: String,
    frontier: String,
    policy_digest: String,
    period_start: i64,
    run_fence: execution::Fence,
    device_fence: execution::Fence,
}
#[derive(Serialize, Deserialize)]
struct Capture {
    binding: Binding,
    result: ModelAttemptResult,
}
struct Session<'a> {
    runtime: &'a RuntimeAuthority,
    step: &'a VerifiedStep,
    admitted: Mutex<Option<Binding>>,
}

#[async_trait]
impl DispatchAuthority for Session<'_> {
    async fn admit(&self, request: &DispatchRequest) -> Result<Admission, AdmissionError> {
        let result: Result<Admission, RuntimeError> = async {
            self.runtime.continuity()?;
            if request.context != self.step.request.context {
                return Err(RuntimeError::InvalidBinding);
            }
            let binding = Binding {
                context: request.context.clone(),
                device_cert: self.step.device_cert.clone(),
                frontier: self.step.frontier.clone(),
                period_start: self.step.period_start,
                policy_digest: self.step.policy_digest.clone(),
                request_digest: request.request_digest.clone(),
                requested_output_tokens: request.descriptor.requested_output_tokens,
                hold_units: self.runtime.price.quote(request)?,
                price: self.runtime.price.clone(),
            };
            self.runtime
                .execution
                .prepare(self.step, &binding, &self.runtime.custody)
                .await?;
            self.runtime.finance.reserve(&binding).await?;
            #[cfg(test)]
            if self.runtime.fault.load(Ordering::SeqCst) == 4 {
                return Err(RuntimeError::Unavailable);
            }
            let Some(claim) = self.runtime.finance.claim(&binding).await? else {
                return Ok(Admission::AlreadyKnown);
            };
            #[cfg(test)]
            if self.runtime.fault.load(Ordering::SeqCst) == 1 {
                return Err(RuntimeError::Unavailable);
            }
            let guard = self.runtime.finance.guard(&binding).await?;
            let admission = self
                .runtime
                .execution
                .admit_once(self.step, claim, &binding, &self.runtime.custody)
                .await?;
            #[cfg(test)]
            if self.runtime.fault.load(Ordering::SeqCst) == 2 {
                return Err(RuntimeError::Unavailable);
            }
            #[cfg(test)]
            if self.runtime.fault.load(Ordering::SeqCst) == 3 {
                guard.rollback().await?;
                return Err(RuntimeError::Unavailable);
            }
            let permit = guard.finish(admission).await?;
            *self
                .admitted
                .lock()
                .map_err(|_| RuntimeError::Unavailable)? = Some(binding);
            permit.consume()
        }
        .await;
        result.map_err(|e| match e {
            RuntimeError::Denied
            | RuntimeError::OverBudget
            | RuntimeError::InvalidBinding
            | RuntimeError::DeploymentMismatch
            | RuntimeError::InvalidPolicy
            | RuntimeError::InvalidProfile => AdmissionError::Denied,
            _ => AdmissionError::Unavailable,
        })
    }
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'/'))
}

#[cfg(test)]
mod tests;
