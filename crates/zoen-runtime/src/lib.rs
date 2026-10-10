//! Postgres finance and fenced native FDB execution. Production work remains
//! closed until managed custody and external restore continuity are available.
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

#[derive(Debug, PartialEq, Eq)]
pub enum ModelCancellation {
    Requested,
    Released,
    Retained { financial: FinancialState },
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
        let result = self.complete_verified(&step).await;
        // Lease loss must not erase an already incurred and retained bill.
        let _ = step.release(&self.execution).await;
        result.map(|(_, financial)| RetainedModel { financial })
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
    async fn verified_step(&self, run: &str) -> Result<VerifiedStep, RuntimeError> {
        let native = self
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        self.continuity()?;
        Ok(VerifiedStep::Native(Box::new(
            native.verified_reply(self, run).await?,
        )))
    }
    pub async fn cancel_model(&self, run: &str) -> Result<ModelCancellation, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.continuity()?;
        self.execution
            .request_reply_cancel(run, &self.custody, self)
            .await?;
        let native::CleanupAcquisition::Ready(step) =
            self.execution.cleanup_reply(run, &self.custody).await?
        else {
            return Ok(ModelCancellation::Requested);
        };
        let result = async {
            match self
                .execution
                .close_reply_before_dispatch(&step, &self.custody, self)
                .await?
            {
                Some(proof) => {
                    self.finance.release(proof, self).await?;
                    Ok(ModelCancellation::Released)
                }
                None => Ok(ModelCancellation::Retained {
                    financial: self
                        .finance
                        .inspect(&step.binding().context.attempt_id)
                        .await?
                        .ok_or(RuntimeError::InvalidBinding)?,
                }),
            }
        }
        .await;
        let _ = step.release(&self.execution).await;
        result
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
            .complete(step.request().clone(), &session)
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
enum VerifiedStep {
    Native(Box<native::ReplyStep>),
    #[cfg(test)]
    Fixture(Box<FixtureStep>),
}

#[cfg(test)]
struct FixtureStep {
    request: ModelRequest,
    device_cert: String,
    frontier: String,
    policy_digest: String,
    period_start: i64,
    run_fence: execution::Fence,
    device_fence: execution::Fence,
}

impl VerifiedStep {
    fn request(&self) -> &ModelRequest {
        match self {
            Self::Native(step) => step.request(),
            #[cfg(test)]
            Self::Fixture(step) => &step.request,
        }
    }

    fn binding(
        &self,
        request: &DispatchRequest,
        _price: &PriceProfile,
    ) -> Result<Binding, RuntimeError> {
        if request.context != self.request().context {
            return Err(RuntimeError::InvalidBinding);
        }
        match self {
            Self::Native(step) => step.binding(request),
            #[cfg(test)]
            Self::Fixture(step) => Ok(Binding {
                context: request.context.clone(),
                device_cert: step.device_cert.clone(),
                frontier: step.frontier.clone(),
                period_start: step.period_start,
                policy_digest: step.policy_digest.clone(),
                request_digest: request.request_digest.clone(),
                requested_output_tokens: request.descriptor.requested_output_tokens,
                hold_units: _price.quote(request)?,
                price: _price.clone(),
            }),
        }
    }

    fn grant_deadline(&self) -> Option<i64> {
        match self {
            Self::Native(step) => step.grant_deadline(),
            #[cfg(test)]
            Self::Fixture(_) => None,
        }
    }

    async fn release(&self, execution: &execution::Execution) -> Result<(), RuntimeError> {
        match self {
            Self::Native(step) => step.release(execution).await,
            #[cfg(test)]
            Self::Fixture(_) => Ok(()),
        }
    }

    #[cfg(test)]
    fn fixture(&self) -> &FixtureStep {
        match self {
            Self::Fixture(step) => step,
            Self::Native(_) => panic!("native authority is not a seeded financial fixture"),
        }
    }
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
            let binding = self.step.binding(request, &self.runtime.price)?;
            self.runtime
                .execution
                .prepare(self.step, &binding, &self.runtime.custody)
                .await?;
            #[cfg(test)]
            if self.runtime.fault.load(Ordering::SeqCst) == 21 {
                self.runtime.native_cut_entered.notify_one();
                self.runtime.native_cut_resume.notified().await;
            }
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
            let guard = self.runtime.finance.guard(&binding, self.step).await?;
            let admission = self
                .runtime
                .execution
                .admit_once(
                    self.step,
                    claim,
                    &binding,
                    &self.runtime.custody,
                    self.runtime,
                )
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
            #[cfg(test)]
            if self.runtime.fault.load(Ordering::SeqCst) == 24 {
                self.runtime.native_cut_entered.notify_one();
                self.runtime.native_cut_resume.notified().await;
            }
            let permit = guard.finish(admission).await?;
            #[cfg(test)]
            if self.runtime.fault.load(Ordering::SeqCst) == 23 {
                return Err(RuntimeError::Unavailable);
            }
            #[cfg(test)]
            if self.runtime.fault.load(Ordering::SeqCst) == 25 {
                self.runtime.native_cut_entered.notify_one();
                self.runtime.native_cut_resume.notified().await;
            }
            self.runtime.continuity()?;
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
