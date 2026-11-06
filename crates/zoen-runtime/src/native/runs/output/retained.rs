//! An original completed result is observational, never another dispatch permit.
use super::*;

pub(crate) async fn retained_model_state(
    runtime: &RuntimeAuthority,
    run: &str,
) -> Result<Option<crate::FinancialState>, RuntimeError> {
    let scope = runtime.production_output_scope()?;
    let tx = runtime.execution.transaction().await?;
    scope.check_in(&runtime.execution, &tx).await?;
    let (record, original) = runtime
        .execution
        .reply_in(&tx, run, &runtime.custody)
        .await?;
    let principal = &record.original.context.principal;
    scope.check_device(&principal.agent, &principal.device)?;
    if tx
        .get(
            &runtime
                .execution
                .attempt(&record.locator.attempt, "native-capture/1"),
            false,
        )
        .await?
        .is_none()
    {
        return Ok(None);
    }
    let retained = runtime
        .execution
        .output_capture_in(&tx, &record, &original, &runtime.custody)
        .await?;
    let expected = match record
        .binding
        .price
        .chat()?
        .charge(&retained.capture.result)?
    {
        Some(units) => crate::FinancialState::Settled { units },
        None => crate::FinancialState::Claimed,
    };
    let observed = runtime
        .finance
        .inspect(&record.locator.attempt)
        .await?
        .ok_or(RuntimeError::RestoreUnreconciled)?;
    if observed != expected {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    // The enum carries no permission. Still refuse a mismatching current pair,
    // altered result or accounting rather than infer completion from a nonce.
    runtime.check_shared_runtime().await?;
    Ok(Some(observed))
}
