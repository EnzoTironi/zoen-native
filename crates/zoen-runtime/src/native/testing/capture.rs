//! Actual retained native facts and the ordinary shared one-shot admission.
//! The only transport here is the explicit local fixture gateway; no production
//! continuity Boolean, fixture claim or output authority is installed.
use super::*;
use crate::{finance::FreshClaim, VerifiedStep};
use relay_admission::managed_images::GenerationGuard;
use std::sync::Mutex;
struct Adapter<'a> {
    runtime: &'a RuntimeAuthority,
    step: &'a VerifiedStep,
    claim: Mutex<Option<FreshClaim>>,
    generation: Mutex<Option<GenerationGuard>>,
}
#[async_trait::async_trait]
impl zoen_models::DispatchAuthority for Adapter<'_> {
    async fn admit(
        &self,
        request: &zoen_models::DispatchRequest,
    ) -> Result<zoen_models::Admission, zoen_models::AdmissionError> {
        let result: Result<zoen_models::Admission, RuntimeError> = async {
            let binding = self.step.binding(request, &self.runtime.price)?;
            let claim = self
                .claim
                .lock()
                .map_err(|_| RuntimeError::Unavailable)?
                .take()
                .ok_or(RuntimeError::Denied)?;
            let completed = self
                .runtime
                .dispatch_verified_budget(self.step, claim, &binding)
                .await?;
            *self
                .generation
                .lock()
                .map_err(|_| RuntimeError::Unavailable)? = Some(completed.generation);
            completed.permit.consume()
        }
        .await;
        result.map_err(|_| zoen_models::AdmissionError::Unavailable)
    }
}
pub(crate) async fn complete_retained_for_capture(
    runtime: &RuntimeAuthority,
    run: &str,
    claim: FreshClaim,
) -> Result<crate::Capture, RuntimeError> {
    let step = VerifiedStep::Native(Box::new(retained_reply_holder(runtime, run).await?));
    let result: Result<crate::Capture, RuntimeError> = async {
        let descriptor = runtime
            .gateway
            .preflight(step.request())
            .map_err(|_| RuntimeError::InvalidProfile)?;
        let binding = step.binding(&descriptor, &runtime.price)?;
        let adapter = Adapter {
            runtime,
            step: &step,
            claim: Mutex::new(Some(claim)),
            generation: Mutex::new(None),
        };
        let result = runtime
            .gateway
            .complete(step.request().clone(), &adapter)
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        Ok(crate::Capture { binding, result })
    }
    .await;
    let released = step.release(&runtime.execution).await;
    let capture = result?;
    released?;
    Ok(capture)
}
