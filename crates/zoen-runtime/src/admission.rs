//! Existing chat admission and immutable binding; no public constructor.
use super::*;
use async_trait::async_trait;
use std::sync::Mutex;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Binding {
    pub(super) context: AttemptContext,
    pub(super) device_cert: String,
    pub(super) frontier: String,
    pub(super) period_start: i64,
    pub(super) policy_digest: String,
    pub(super) request_digest: String,
    pub(super) requested_output_tokens: u64,
    pub(super) hold_units: i64,
    pub(super) price: PriceProfile,
}
impl Binding {
    pub(super) fn digest(&self) -> Result<String, RuntimeError> {
        Ok(hash(
            &serde_json::to_vec(self).map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
}
// Private and deliberately lacks Deserialize/general-purpose constructor.
// Only actual core extraction may create one in production; fixtures live in cfg(test).
pub(super) enum VerifiedStep {
    Native(Box<native::ReplyStep>),
    #[cfg(test)]
    Fixture(Box<FixtureStep>),
}

#[cfg(test)]
pub(super) struct FixtureStep {
    pub(super) request: ModelRequest,
    pub(super) device_cert: String,
    pub(super) frontier: String,
    pub(super) policy_digest: String,
    pub(super) period_start: i64,
    pub(super) run_fence: execution::Fence,
    pub(super) device_fence: execution::Fence,
}

impl VerifiedStep {
    pub(super) fn request(&self) -> &ModelRequest {
        match self {
            Self::Native(step) => step.request(),
            #[cfg(test)]
            Self::Fixture(step) => &step.request,
        }
    }

    pub(super) fn binding(
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

    pub(super) fn grant_deadline(&self) -> Option<i64> {
        match self {
            Self::Native(step) => step.grant_deadline(),
            #[cfg(test)]
            Self::Fixture(_) => None,
        }
    }

    pub(super) async fn release(
        &self,
        execution: &execution::Execution,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Native(step) => step.release(execution).await,
            #[cfg(test)]
            Self::Fixture(_) => Ok(()),
        }
    }

    #[cfg(test)]
    pub(super) fn fixture(&self) -> &FixtureStep {
        match self {
            Self::Fixture(step) => step,
            Self::Native(_) => panic!("native authority is not a seeded financial fixture"),
        }
    }
}
#[derive(Serialize, Deserialize)]
pub(super) struct Capture {
    pub(super) binding: Binding,
    pub(super) result: ModelAttemptResult,
}
pub(super) struct Session<'a> {
    pub(super) runtime: &'a RuntimeAuthority,
    pub(super) step: &'a VerifiedStep,
    pub(super) admitted: Mutex<Option<AdmittedBinding>>,
}

pub(super) struct AdmittedBinding {
    pub(super) binding: Binding,
    pub(super) nonce: String,
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
            let nonce = admission.nonce().to_string();
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
                .map_err(|_| RuntimeError::Unavailable)? = Some(AdmittedBinding { binding, nonce });
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
