//! Revocation while a decision provider is outside all native/FDB leases.
use super::*;
use std::sync::{mpsc, Mutex};

struct PausingJudge {
    point: gate::Point,
    entered: Arc<tokio::sync::Notify>,
    resume: Mutex<mpsc::Receiver<()>>,
}
impl DecisionProvider for PausingJudge {
    fn name(&self) -> String {
        "paused-policy-provider".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        let outbound = request.questions.iter().any(|question| {
            matches!(question,
            gate::Question::Score { name, .. } if name == "harmful_content")
        });
        if outbound == (self.point == gate::Point::Outbound) {
            self.entered.notify_one();
            self.resume
                .lock()
                .map_err(|_| GateError::Unavailable("mutex".into()))?
                .recv()
                .map_err(|_| GateError::Unavailable("closed".into()))?;
        }
        gate::Mock.decide(request)
    }
}
pub(super) async fn run() -> TestResult {
    for point in [gate::Point::ConversationMessage, gate::Point::Outbound] {
        Box::pin(pause(point)).await?;
    }
    Ok(())
}
async fn pause(point: gate::Point) -> TestResult {
    let (mut f, relay, run) = Box::pin(ready()).await?;
    answer(&f, "Uma resposta comum para a conversa.")?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let (resume, receiver) = mpsc::channel();
    let consumer = ReplyRuntimeHost::configured(
        f.world.runtime.clone(),
        ReplyHostConfig {
            agent: f.agent.id.clone(),
            device: f.world.agents[0].device.clone().ok_or("device")?,
            target: relay.target(&f, relay.address),
        },
        gate::Gate {
            on_device: Some(Box::new(PausingJudge {
                point,
                entered: entered.clone(),
                resume: Mutex::new(receiver),
            })),
            ..gate::Gate::default()
        },
    )?;
    let change = async {
        entered.notified().await;
        let leases =
            native::testing::output::policy_leases_released(&f.world.runtime, &run.run).await;
        let result = zoen_relay::agents::control(
            &f.world.runtime.finance.pool,
            &f.owner.id,
            &f.agent.id,
            protocol::AgentAction::Pause,
        )
        .await;
        // Always unblock the provider, including a failed control request.
        resume.send(()).map_err(|_| "resume channel")?;
        result?;
        assert!(leases?);
        TestResult::Ok(())
    };
    let (result, changed) = tokio::join!(Box::pin(consumer.advance(&run.run)), change);
    changed?;
    assert!(
        matches!(result, Err(RuntimeError::Denied)),
        "revoked decision: {result:?}"
    );
    let sends = usize::from(point == gate::Point::Outbound);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), sends);
    assert_eq!(
        f.world.balance().await.spent_units,
        if sends == 1 { 6 } else { 0 }
    );
    assert_eq!(
        f.log
            .read(&f.space, 1, 256)
            .await?
            .iter()
            .filter(|event| event.env.author() == f.agent.id)
            .count(),
        0
    );
    drop(consumer);
    zoen_relay::agents::control(
        &f.world.runtime.finance.pool,
        &f.owner.id,
        &f.agent.id,
        protocol::AgentAction::Resume,
    )
    .await?;
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let consumer = host(&f, &relay, None, Arc::new(AtomicUsize::new(0)))?;
    assert!(matches!(
        Box::pin(consumer.advance(&run.run)).await?,
        ReplyHostProgress::Output {
            delivery: crate::ReplyReceiptProgress::Accepted { .. },
            ..
        }
    ));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("native reply host {point:?}: authority paused during provider decision, no unauthorized model/output, cold resume keeps original incurred result PASS");
    Ok(())
}
