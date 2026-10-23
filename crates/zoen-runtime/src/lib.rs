//! Postgres finance and fenced native FDB execution. Production work remains
//! closed until external restore continuity and output admission are available.
mod admission;
#[cfg(test)]
use admission::FixtureStep;
use admission::{Binding, Capture, Session, VerifiedStep};
mod custody;
mod execution;
mod finance;
mod image_tasks;
mod native;
pub use native::ImageOutputInspection;
mod pricing;
pub use pricing::{ImagePriceProfile, ImagePriceQuote, PriceProfile};
pub use types::owner_budget::{OwnerPeriodPolicy, SignedOwnerPolicy};

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

/// Historical initial publication state, not current relay stock or permission.
#[derive(Debug, PartialEq, Eq)]
pub struct NativeKeyPackagePublication {
    pub native: NativeInspection,
    pub packages: u32,
    pub published: bool,
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

#[derive(Debug, PartialEq, Eq)]
pub struct ReplyOutputInspection {
    pub client_id: String,
    pub message: String,
    pub generation: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReplyReceiptProgress {
    Waiting,
    CatchingUp {
        through: types::Seen,
        generation: u64,
    },
    Accepted {
        accepted: types::Seen,
        generation: u64,
    },
}

pub struct RuntimeAuthority {
    finance: finance::Finance,
    execution: execution::Execution,
    custody: custody::Custody,
    // Native managed construction resolves a certified device from SQL and the
    // existing vault contract; it does not grant continuity or output authority.
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
    #[cfg(test)]
    output_fixture: Option<native::FreshOutputNamespace>,
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
            #[cfg(test)]
            output_fixture: None,
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
        #[cfg(test)]
        println!("native paid stage: verified_step_start");
        let step = self.verified_step(run).await?;
        #[cfg(test)]
        println!("native paid stage: verified_step_complete");
        let result = self.complete_verified(&step).await;
        #[cfg(test)]
        println!(
            "native paid stage: complete_verified result={:?}",
            result.as_ref().map(|_| "ok")
        );
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

    fn production_output_scope(&self) -> Result<&native::FreshOutputNamespace, RuntimeError> {
        // A fresh fixture cannot attest that old production writers stopped.
        Err(RuntimeError::CoreAuthorityUnavailable)
    }

    /// Closed until actual custody, continuity and old-writer barrier evidence
    /// can authorize this boundary. A locator never creates that evidence.
    pub async fn prepare_reply_output(
        &self,
        run: &str,
    ) -> Result<ReplyOutputInspection, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        let scope = self.production_output_scope()?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .prepare_output(self, run, scope)
            .await
    }

    pub async fn reconcile_reply_receipt(
        &self,
        run: &str,
    ) -> Result<ReplyReceiptProgress, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        let scope = self.production_output_scope()?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .reconcile_output(self, run, scope)
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
            if self
                .capture_retained_evidence(&attempt, &digest, &sealed)
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
            image: None,
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
        let admitted = session
            .admitted
            .lock()
            .map_err(|_| RuntimeError::Unavailable)?
            .take()
            .ok_or(RuntimeError::InvalidBinding)?;
        let capture = Capture {
            binding: admitted.binding,
            result: result.clone(),
        };
        let (digest, sealed) = self.custody.seal(
            &format!("evidence/{}", capture.binding.context.attempt_id),
            &capture,
        )?;
        // Output association refusal must not prevent either independent store
        // from retaining an already incurred expense, including late captures.
        let _ = native::associate_capture(self, step, &capture, &admitted.nonce, &digest, &sealed)
            .await;
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
