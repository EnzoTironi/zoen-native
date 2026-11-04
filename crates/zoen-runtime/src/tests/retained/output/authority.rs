//! The shared output interface must not turn a retained, already-paid result
//! into publication permission. Exercise the real native/SQL/FDB cohort.
use super::*;
use async_trait::async_trait;
use foundationdb::Transaction;
use native::NativeOutputAuthority;

struct RefusedOutput;
#[async_trait]
impl NativeOutputAuthority for RefusedOutput {
    async fn check_in(
        &self,
        _: &execution::Execution,
        _: &Transaction,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::Denied)
    }
    fn check_context(&self, _: &RuntimeAuthority) -> Result<(), RuntimeError> {
        Err(RuntimeError::Denied)
    }
    fn check_device(&self, _: &str, _: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Denied)
    }
    fn check_target(&self, _: &ImageDeliveryTarget) -> Result<(), RuntimeError> {
        Err(RuntimeError::Denied)
    }
}

pub(super) async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (fixture, run) = original().await?;
    let runtime = &fixture.world.runtime;
    runtime.run_model(&run.run).await?;
    assert_one_bill(&fixture).await;
    let actor = &fixture.world.agents[0];
    let device = actor.device.as_deref().ok_or("fixture device")?;
    let generation = native::testing::generation(runtime, &actor.identity, device).await;
    let custody = runtime.native.as_ref().ok_or("fixture custody")?;
    assert_eq!(
        custody
            .prepare_output(runtime, &run.run, &RefusedOutput)
            .await,
        Err(RuntimeError::Denied)
    );
    assert_eq!(
        custody
            .reconcile_output(runtime, &run.run, &RefusedOutput)
            .await,
        Err(RuntimeError::Denied)
    );
    assert_eq!(
        native::testing::generation(runtime, &actor.identity, device).await,
        generation
    );
    assert_one_bill(&fixture).await;
    assert!(matches!(
        runtime.prepare_reply_output(&run.run).await,
        Err(RuntimeError::CoreAuthorityUnavailable)
    ));
    let inspection = proof::prepare(runtime, &run.run).await?;
    assert!(!inspection.message.is_empty());
    assert_one_bill(&fixture).await;
    println!("shared native output authority: paid capture does not override denied publication/receipt or open production from a fixture; native generation and one bill preserved PASS");
    fixture.world.finish().await;
    Ok(())
}
