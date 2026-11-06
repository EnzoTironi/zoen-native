//! Actual native host decisions surround one paid inference and MLS publication.
use super::*;
use crate::{ReplyHostConfig, ReplyHostProgress, ReplyRuntimeHost};
use gate::{DecisionProvider, DecisionRequest, DecisionResponse, GateError};
use std::sync::atomic::AtomicUsize;

struct Judge {
    calls: Arc<AtomicUsize>,
    hold: Option<gate::Point>,
}
impl DecisionProvider for Judge {
    fn name(&self) -> String {
        "controlled-policy-provider".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let outbound = request.questions.iter().any(|question| {
            matches!(question,
            gate::Question::Score { name, .. } if name == "harmful_content")
        });
        if self.hold
            == Some(if outbound {
                gate::Point::Outbound
            } else {
                gate::Point::ConversationMessage
            })
        {
            return Err(GateError::Unavailable(
                "controlled unavailable decision".into(),
            ));
        }
        gate::Mock.decide(request)
    }
}
fn host(
    f: &Fixture,
    relay: &Relay,
    hold: Option<gate::Point>,
    calls: Arc<AtomicUsize>,
) -> Result<ReplyRuntimeHost, RuntimeError> {
    ReplyRuntimeHost::configured(
        f.world.runtime.clone(),
        ReplyHostConfig {
            agent: f.agent.id.clone(),
            device: f.world.agents[0]
                .device
                .clone()
                .ok_or(RuntimeError::InvalidBinding)?,
            target: relay.target(f, relay.address),
        },
        gate::Gate {
            on_device: Some(Box::new(Judge { calls, hold })),
            ..gate::Gate::default()
        },
    )
}
fn answer(f: &Fixture, text: &str) -> TestResult {
    f.world.http.body.lock().map_err(|_| "body")?["choices"][0]["message"]["content"] = text.into();
    Ok(())
}
pub(in crate::tests) async fn run() -> TestResult {
    Box::pin(happy()).await?;
    for point in [gate::Point::ConversationMessage, gate::Point::Outbound] {
        Box::pin(held(point)).await?;
    }
    Box::pin(blocked_retained_secret()).await?;
    Box::pin(unusable()).await?;
    Box::pin(races::run()).await?;
    Ok(())
}
mod races;
async fn happy() -> TestResult {
    let (mut f, relay, run) = Box::pin(ready()).await?;
    answer(&f, "Aqui está uma resposta para a sua pergunta.")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let wrong = ReplyHostConfig {
        agent: f.agent.id.clone(),
        device: types::new_id("wrong")?,
        target: relay.target(&f, relay.address),
    };
    assert!(
        ReplyRuntimeHost::configured(f.world.runtime.clone(), wrong, gate::Gate::default())
            .is_err()
    );
    let mut target = relay.target(&f, relay.address);
    target.source = types::new_id("wrong")?;
    let wrong = ReplyHostConfig {
        agent: f.agent.id.clone(),
        device: f.world.agents[0].device.clone().ok_or("device")?,
        target,
    };
    assert!(
        ReplyRuntimeHost::configured(f.world.runtime.clone(), wrong, gate::Gate::default())
            .is_err()
    );
    let consumer = host(&f, &relay, None, calls.clone())?;
    assert!(consumer.advance(&types::new_id("missing")?).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    let (client_id, audit) = match Box::pin(consumer.advance(&run.run)).await? {
        ReplyHostProgress::Output {
            output,
            delivery: crate::ReplyReceiptProgress::Accepted { .. },
            financial,
            audit,
            ..
        } => {
            assert_eq!(financial, crate::FinancialState::Settled { units: 6 });
            (output.client_id, audit)
        }
        progress => return Err(format!("expected accepted reply: {progress:?}").into()),
    };
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(audit.point, "outbound");
    assert_eq!(audit.outcome, "allow");
    assert_eq!(
        audit.input_hash,
        crate::hash("Aqui está uma resposta para a sua pergunta.".as_bytes())
    );
    assert_eq!(
        native::testing::output::policy_audit(&f.world.runtime, &run.run).await?,
        Some(audit)
    );
    zoen_relay::agents::control(
        &f.world.runtime.finance.pool,
        &f.owner.id,
        &f.agent.id,
        protocol::AgentAction::Pause,
    )
    .await?;
    drop(consumer);
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let consumer = host(&f, &relay, Some(gate::Point::Outbound), calls.clone())?;
    assert!(matches!(
        Box::pin(consumer.advance(&run.run)).await?,
        ReplyHostProgress::Completed {
            delivery: crate::ReplyReceiptProgress::Accepted { .. },
            ..
        }
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(
        f.log
            .read(&f.space, 1, 256)
            .await?
            .iter()
            .filter(|event| event.env.client_id() == client_id)
            .count(),
        1
    );
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("native reply host: real source, inbound/outbound Gate, paid HTTP, durable audit, MLS relay, cold accepted retry without new decision/inference/charge PASS");
    Ok(())
}
async fn held(point: gate::Point) -> TestResult {
    let (mut f, relay, run) = Box::pin(ready()).await?;
    answer(&f, "Resposta retida para revisão.")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let consumer = host(&f, &relay, Some(point), calls.clone())?;
    for _ in 0..2 {
        let ReplyHostProgress::Held {
            audit, financial, ..
        } = Box::pin(consumer.advance(&run.run)).await?
        else {
            return Err("expected held reply".into());
        };
        assert_ne!(audit.outcome, "allow");
        if point == gate::Point::ConversationMessage {
            assert_eq!(financial, None);
        } else {
            assert_eq!(financial, Some(crate::FinancialState::Settled { units: 6 }));
        }
    }
    let sends = usize::from(point == gate::Point::Outbound);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), sends);
    assert_eq!(
        f.world.balance().await.spent_units,
        if sends == 1 { 6 } else { 0 }
    );
    drop(consumer);
    // Retry the same retained run after policy recovery, including a cold restart.
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let consumer = host(&f, &relay, None, calls)?;
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
    println!("native reply host {point:?}: held policy, original retained, cold recovery, one paid inference PASS");
    Ok(())
}

async fn unusable() -> TestResult {
    let (mut f, relay, run) = Box::pin(ready()).await?;
    answer(&f, "   ")?;
    for cold in [false, true] {
        if cold {
            reopen(&mut f).await?;
            f = Box::pin(execution::qualify(f, &relay)).await?;
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let consumer = host(&f, &relay, None, calls.clone())?;
        assert!(matches!(
            Box::pin(consumer.advance(&run.run)).await?,
            ReplyHostProgress::AwaitingRecovery {
                financial: crate::FinancialState::Settled { units: 6 },
                ..
            }
        ));
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(!cold));
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
        assert_eq!(f.world.balance().await.spent_units, 6);
        drop(consumer);
    }
    relay.stop().await;
    f.finish().await;
    println!("native reply host unusable paid response: no output or repeated inference across cold recovery PASS");
    Ok(())
}

async fn blocked_retained_secret() -> TestResult {
    let (mut f, relay, run) = Box::pin(ready()).await?;
    answer(&f, &format!("sk-{}", "a".repeat(32)))?;
    for cold in [false, true] {
        if cold {
            reopen(&mut f).await?;
            f = Box::pin(execution::qualify(f, &relay)).await?;
            // Changed provider configuration cannot replace the paid original.
            answer(&f, "A harmless new response")?;
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let consumer = host(&f, &relay, None, calls.clone())?;
        let ReplyHostProgress::Held {
            audit, financial, ..
        } = Box::pin(consumer.advance(&run.run)).await?
        else {
            return Err("retained credential must remain blocked".into());
        };
        assert_eq!(audit.outcome, "block");
        assert!(!audit.summary.contains(&"a".repeat(32)));
        assert_eq!(financial, Some(crate::FinancialState::Settled { units: 6 }));
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(!cold));
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
        assert_eq!(f.world.balance().await.spent_units, 6);
        drop(consumer);
    }
    relay.stop().await;
    f.finish().await;
    println!("native reply host secret: hard outbound block, redacted audit, same paid original after cold restart and provider changes PASS");
    Ok(())
}
