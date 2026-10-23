//! Controlled provider adapter; real existing SQL/FDB finance, never paid HTTP.
use super::*;
use crate::{
    image_tasks::{ImageAttemptAdapter, ImageTask},
    pricing::BoundPrice,
};
use async_trait::async_trait;
use serde_json::json;
use zoen_models::*;

fn profile() -> ImagePriceProfile {
    ImagePriceProfile {
        version: "image-fixture-price-v1".into(),
        endpoint: "https://api.openai.com/v1/images/generations".into(),
        model: "gpt-image-fixture".into(),
        credential_ref: "image-fixture".into(),
        currency: "USD".into(),
        scale: 6,
        image: ImageDispatch {
            count: 1,
            size: ImageSize::Square,
            quality: ImageQuality::Low,
            background: ImageBackground::Opaque,
            format: "png".into(),
            max_image_bytes: 1_000_000,
        },
        input_text_units_per_million: 1_000_000,
        input_image_units_per_million: 2_000_000,
        output_text_units_per_million: 3_000_000,
        output_image_units_per_million: 4_000_000,
        reservation_units: 10,
        max_request_bytes: 32768,
        max_response_bytes: 1_000_000,
    }
}
async fn world() -> World {
    let mut w = World::with_relay().await;
    let capability = native::testing::output::provision(&w.runtime).await;
    Arc::get_mut(&mut w.runtime).unwrap().output_fixture = Some(capability);
    let mut policy = w.signed.policy.clone();
    policy.version += 1;
    policy.previous_digest = Some(w.policy_digest.clone());
    policy.allowed_profiles.push(profile().digest().unwrap());
    policy.allowed_profiles.sort();
    assert!(
        policy.valid_shape(),
        "fixture policy preserves canonical profile order"
    );
    w.signed = event_log::owner_budget::sign(&w.owner, policy).unwrap();
    w.policy_digest = w.runtime.install_policy(w.signed.clone()).await.unwrap();
    w
}
async fn image_task(w: &World) -> (VerifiedStep, ImageTask) {
    let mut step = w.step(0).await;
    let VerifiedStep::Fixture(fixture) = &mut step else {
        panic!("controlled scope only")
    };
    fixture.request.operation = Operation::ImageGeneration;
    fixture.request.max_output_tokens = 0;
    fixture.request.context.price_version = profile().version;
    w.runtime
        .execution
        .seed(&step, &w.runtime.custody)
        .await
        .unwrap();
    let request = step.request();
    let p = profile();
    let dispatch = DispatchDescriptor {
        operation: Operation::ImageGeneration,
        endpoint: p.endpoint.clone(),
        model: p.model.clone(),
        credential_ref: p.credential_ref.clone(),
        encoded_request_bytes: 200,
        max_request_bytes: 32768,
        max_response_bytes: p.max_response_bytes,
        requested_output_tokens: 0,
        image: Some(p.image.clone()),
    };
    let mut binding = w.binding(&step);
    binding.context = request.context.clone();
    binding.requested_output_tokens = 0;
    binding.hold_units = 10;
    binding.price = BoundPrice::Image(p);
    let mut task = ImageTask {
        schema: 1,
        request: ImageRequest {
            context: request.context.clone(),
            prompt: PROMPT.into(),
            size: ImageSize::Square,
            quality: ImageQuality::Low,
            background: ImageBackground::Opaque,
            max_image_bytes: 1_000_000,
        },
        dispatch,
        binding,
        source_digest: "a".repeat(64),
        task_digest: String::new(),
        request_id: "controlled-image-request".into(),
        approval_hash: "b".repeat(64),
    };
    task.task_digest = task.expected_digest().unwrap();
    task.validate().unwrap();
    (step, task)
}
fn result(task: &ImageTask, known: bool) -> ImageAttemptResult {
    let usage = if known {
        json!({"input_tokens":5,"output_tokens":2,"total_tokens":7,"input_tokens_details":{"text_tokens":2,"image_tokens":3},"output_tokens_details":{"text_tokens":1,"image_tokens":1}})
    } else {
        serde_json::Value::Null
    };
    ImageAttemptResult {
        context: task.binding.context.clone(),
        request_digest: task.binding.request_digest.clone(),
        output: Err(OutputFailure::MalformedResponse),
        receipt: ProviderReceipt {
            http_status: Some(200),
            request_id: Some("controlled-provider-id".into()),
            response_id: None,
            reported_model: None,
            usage: if known {
                UsageEvidence::Reported(ReportedUsage {
                    input_tokens: Some(5),
                    output_tokens: Some(2),
                    total_tokens: Some(7),
                    cache_read_tokens: None,
                    reasoning_tokens: None,
                })
            } else {
                UsageEvidence::Missing
            },
        },
        image_usage: if known {
            ImageUsageEvidence::Reported(ImageTokenDetails {
                input_text_tokens: Some(2),
                input_image_tokens: Some(3),
                output_text_tokens: Some(1),
                output_image_tokens: Some(1),
            })
        } else {
            ImageUsageEvidence::Missing
        },
        evidence: ProviderEvidence {
            complete: true,
            body: serde_json::to_vec(&json!({"usage":usage,"padding":"x".repeat(200_000)}))
                .unwrap(),
        },
    }
}
struct Provider {
    task: ImageTask,
    result: ImageAttemptResult,
    sends: std::sync::atomic::AtomicUsize,
    close: Option<PgPool>,
}
#[async_trait]
impl ImageAttemptAdapter for Provider {
    async fn generate(
        &self,
        request: ImageRequest,
        authority: &dyn DispatchAuthority,
    ) -> Result<ImageAttemptResult, GatewayError> {
        assert_eq!(request, self.task.request);
        let dispatch = DispatchRequest {
            context: request.context,
            request_digest: self.task.binding.request_digest.clone(),
            profile: IMAGE_PROFILE,
            descriptor: self.task.dispatch.clone(),
        };
        if authority.admit(&dispatch).await? != Admission::Fresh {
            return Err(GatewayError::NotFresh);
        }
        self.sends.fetch_add(1, Ordering::SeqCst);
        if let Some(pool) = &self.close {
            pool.close().await;
        }
        Ok(self.result.clone())
    }
}
pub(super) async fn run() {
    let mut w = world().await;
    let (step, task) = image_task(&w).await;
    native::testing::seed_association_fixture(&w.runtime, &task).await;
    w.runtime.fault.store(40, Ordering::SeqCst); // suppress association before evidence succeeds
    let provider = Provider {
        result: result(&task, true),
        task: task.clone(),
        sends: Default::default(),
        close: Some(w.runtime.finance.pool.clone()),
    };
    assert_eq!(
        w.runtime
            .complete_image_task(&step, &task, &provider)
            .await
            .unwrap(),
        FinancialState::Claimed,
        "closed SQL retains original incurred evidence in FDB"
    );
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    let reopened = w.reopen().await;
    assert_eq!(
        reopened
            .balance(
                &w.owner.identity,
                w.signed.policy.year,
                w.signed.policy.month
            )
            .await
            .unwrap(),
        PeriodBalance {
            held_units: 10,
            spent_units: 0
        }
    );
    // Actual missing first physical chunk must not settle a partial/corrupt body.
    let trx = reopened.execution.transaction().await.unwrap();
    let transfer = reopened.execution.root.subspace(&("transfer",));
    let rows = trx
        .get_range(&foundationdb::RangeOption::from(transfer.range()), 1, false)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let (attempt, digest): (String, String) = transfer.unpack(rows[0].key()).unwrap();
    let raw = rows[0].value();
    let prefix = b"ZOEN-EVIDENCE-CHUNKS/1\0";
    assert!(raw.starts_with(prefix));
    let manifest: serde_json::Value = serde_json::from_slice(&raw[prefix.len()..]).unwrap();
    let chunk_key = reopened.execution.root.pack(&(
        "evidence-chunk/1",
        attempt.as_str(),
        digest.as_str(),
        manifest["sha256"].as_str().unwrap(),
        0i64,
    ));
    let chunk = trx.get(&chunk_key, false).await.unwrap().unwrap().to_vec();
    trx.clear(&chunk_key);
    trx.commit().await.unwrap();
    assert_eq!(
        reopened.reconcile(64).await.unwrap_err(),
        RuntimeError::Unavailable
    );
    assert_eq!(
        reopened.inspect_attempt(&attempt).await.unwrap(),
        Some(FinancialState::Claimed)
    );
    let trx = reopened.execution.transaction().await.unwrap();
    trx.set(&chunk_key, &chunk);
    trx.commit().await.unwrap();
    assert_eq!(
        reopened
            .execution
            .image_capture_digest(&task, &reopened.custody)
            .await
            .unwrap_err(),
        RuntimeError::Unavailable
    );
    let (recovered_digest, recovered) = reopened.retained_image_capture(&task).await.unwrap();
    assert_eq!(recovered_digest, digest);
    assert!(recovered.task == task);
    assert_eq!(
        reopened
            .execution
            .image_capture_digest(&task, &reopened.custody)
            .await
            .unwrap(),
        digest
    );
    assert_eq!(
        reopened.retained_image_capture(&task).await.unwrap().0,
        digest,
        "repair is idempotent"
    );
    assert_eq!(
        reopened
            .execution
            .associate_image_capture(&task, &"c".repeat(64), &digest, &reopened.custody)
            .await
            .unwrap_err(),
        RuntimeError::InvalidBinding
    );
    reopened.reconcile(64).await.unwrap();
    reopened.reconcile(64).await.unwrap();
    assert_eq!(
        reopened.inspect_attempt(&attempt).await.unwrap(),
        Some(FinancialState::Settled { units: 15 })
    );
    assert_eq!(
        reopened
            .balance(
                &w.owner.identity,
                w.signed.policy.year,
                w.signed.policy.month
            )
            .await
            .unwrap(),
        PeriodBalance {
            held_units: 0,
            spent_units: 15
        }
    );
    assert_eq!(
        provider.sends.load(Ordering::SeqCst),
        1,
        "cold reconciliation never calls provider"
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    w.runtime = reopened;
    drop(provider);
    w.finish().await;

    let w = world().await;
    let (step, task) = image_task(&w).await;
    let provider = Provider {
        result: result(&task, false),
        task: task.clone(),
        sends: Default::default(),
        close: None,
    };
    assert_eq!(
        w.runtime
            .complete_image_task(&step, &task, &provider)
            .await
            .unwrap(),
        FinancialState::Claimed
    );
    assert!(w
        .runtime
        .complete_image_task(&step, &task, &provider)
        .await
        .is_err());
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        w.balance().await,
        PeriodBalance {
            held_units: 10,
            spent_units: 0
        }
    );
    w.runtime.reconcile(64).await.unwrap();
    assert_eq!(
        w.balance().await,
        PeriodBalance {
            held_units: 10,
            spent_units: 0
        }
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_dispositions").await,
        0
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_claims").await, 1);
    // The same SQL table still refuses a large legacy-chat receipt.
    let chat = w.step(1).await;
    let binding = w.binding(&chat);
    w.runtime.finance.reserve(&binding).await.unwrap();
    w.runtime.finance.claim(&binding).await.unwrap().unwrap();
    let error =
        sqlx::query("INSERT INTO runtime_evidence(attempt,digest,sealed) VALUES ($1,$2,$3)")
            .bind(&binding.context.attempt_id)
            .bind("e".repeat(64))
            .bind(vec![1u8; 200_000])
            .execute(&w.runtime.finance.pool)
            .await
            .unwrap_err();
    assert!(matches!(error,sqlx::Error::Database(ref db) if db.code().as_deref()==Some("23514")));
    w.finish().await;
}

#[test]
fn original_chat_price_bytes_and_operation_selection_are_preserved() {
    let price = config("unused", "fixture", "https://provider.example/v1").price;
    let original = serde_json::to_vec(&price).unwrap();
    let bound = BoundPrice::from(price.clone());
    assert_eq!(serde_json::to_vec(&bound).unwrap(), original);
    let reopened: BoundPrice = serde_json::from_slice(&original).unwrap();
    assert_eq!(reopened, BoundPrice::Chat(price));
    assert!(reopened.image().is_err());
    let image = BoundPrice::Image(profile());
    assert!(image.chat().is_err());
    assert_eq!(image.reserve(0).unwrap(), 10);
    assert!(image.reserve(1).is_err());
}
