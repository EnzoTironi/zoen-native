//! Exercise the ordinary live path without enabling production continuity or HTTP.
use super::*;
pub(crate) async fn dispatch_retained_reply(
    runtime: &RuntimeAuthority,
    run: &str,
    claim: crate::finance::FreshClaim,
) -> Result<bool, RuntimeError> {
    let step = crate::VerifiedStep::Native(Box::new(retained_reply_holder(runtime, run).await?));
    let result: Result<bool, RuntimeError> = async {
        let request = runtime
            .gateway
            .preflight(step.request())
            .map_err(|_| RuntimeError::InvalidProfile)?;
        let binding = step.binding(&request, &runtime.price)?;
        let completed = runtime
            .dispatch_verified_budget(&step, claim, &binding)
            .await?;
        let fresh = matches!(completed.permit.consume()?, zoen_models::Admission::Fresh);
        drop(completed.generation);
        Ok(fresh)
    }
    .await;
    let released = step.release(&runtime.execution).await;
    let fresh = result?;
    released?;
    Ok(fresh)
}
