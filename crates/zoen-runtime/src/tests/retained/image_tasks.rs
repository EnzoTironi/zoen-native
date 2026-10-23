//! Real native review, original MLS wire, shared finance and cold FileDoc root.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use client_core::image_tasks::{ImageTaskDefinition, ImageTaskInput};
use native::testing::image_tasks as proof;
use zoen_models::ModelGateway;
mod delivery;
mod provider;

fn gateway(p: &ImagePriceProfile) -> GatewayConfig {
    GatewayConfig {
        base_url: p
            .endpoint
            .strip_suffix("/images/generations")
            .unwrap()
            .into(),
        endpoint_policy: p.endpoint_policy.gateway(),
        model: p.model.clone(),
        credential_ref: p.credential_ref.clone(),
        api_key: "controlled-image-adapter-not-a-provider-secret".into(),
        limits: ModelLimits {
            max_request_bytes: p.max_request_bytes as usize,
            max_response_bytes: 16 * 1024 * 1024,
            max_output_tokens: 8,
            timeout: Duration::from_secs(3),
            connect_timeout: Duration::from_secs(1),
        },
    }
}
fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1024, 1024);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_compression(png::Compression::NoCompression);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![122; 1024 * 1024 * 4])
            .unwrap();
    }
    bytes
}

pub(in crate::tests) async fn run() {
    let mut f = Fixture::with_output().await;
    let original_bytes = png();
    assert!(original_bytes.len() > 4 * 1024 * 1024);
    let provider = provider::Provider::start(&original_bytes).await;
    let mut price = crate::tests::image_tasks::profile();
    price.endpoint = provider.endpoint.clone();
    price.endpoint_policy = crate::ImageEndpointPolicy::LoopbackFixture;
    price.image.max_image_bytes = 8 * 1024 * 1024;
    price.max_response_bytes = 16 * 1024 * 1024;
    let mut policy = f.world.signed.policy.clone();
    policy.version += 1;
    policy.previous_digest = Some(f.world.policy_digest.clone());
    policy.allowed_profiles.push(price.digest().unwrap());
    policy.allowed_profiles.sort();
    f.world.signed = event_log::owner_budget::sign(&f.world.owner, policy).unwrap();
    f.world.policy_digest = f
        .world
        .runtime
        .install_policy(f.world.signed.clone())
        .await
        .unwrap();
    f.encrypted(EventBody::GrantIssued {
        grant: Grant {
            id: "native-image-owner-act".into(),
            grantor: f.owner.id.clone(),
            grantee: Some(f.agent.id.clone()),
            scope: GrantScope::Space(f.space.clone()),
            capability: Capability::Trust(TrustLevel::Act),
            expires_at_ms: None,
        },
    })
    .await;
    f.message("Create one sky image after my explicit review.")
        .await;
    let source = f
        .log
        .read(&f.space, f.head.as_ref().unwrap().seq, 1)
        .await
        .unwrap()
        .remove(0)
        .env
        .client_id()
        .to_string();
    f.sync(false).await.unwrap();
    let actor = f.world.agents[0].clone();
    let device = actor.device.as_deref().unwrap();
    assert_eq!(
        f.world
            .runtime
            .stage_image_task_review(
                crate::ImageReviewIntent {
                    agent: actor.identity.clone(),
                    device: device.into(),
                    space: f.space.clone(),
                    source: source.clone(),
                    input: ImageTaskInput {
                        prompt: "Must not stage through an unqualified output scope".into(),
                        definition: ImageTaskDefinition {
                            version: "native-image-fixture-v1".into(),
                            price_digest: price.digest().unwrap(),
                            size: "1024x1024".into(),
                            quality: "low".into(),
                            background: "opaque".into(),
                            max_image_bytes: price.image.max_image_bytes,
                        }
                    },
                },
                crate::ImageDeliveryTarget::configured(
                    "http://127.0.0.1:1".into(),
                    "uncontacted-fixture".into(),
                    f.world.relay_cell.clone().unwrap()
                )
                .unwrap(),
            )
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable,
    );
    let review_intent = crate::ImageReviewIntent {
        agent: actor.identity.clone(),
        device: device.into(),
        space: f.space.clone(),
        source,
        input: ImageTaskInput {
            prompt: "A quiet blue sky".into(),
            definition: ImageTaskDefinition {
                version: "native-image-fixture-v1".into(),
                price_digest: price.digest().unwrap(),
                size: "1024x1024".into(),
                quality: "low".into(),
                background: "opaque".into(),
                max_image_bytes: price.image.max_image_bytes,
            },
        },
    };
    let (request, approval_hash, _) = delivery::review(&mut f, review_intent).await;
    f.encrypted(EventBody::RequestResolved {
        request: request.clone(),
        approved: true,
        content_hash: approval_hash,
        resolution: Some(RequestResolution {
            message: String::new(),
            created_item: None,
            ai_cost_cents: 0,
            standing_grant: None,
            tool_decision: Some(ToolDecision::ApproveOnce),
        }),
    })
    .await;
    let before_runs = f.world.runtime.pending_reply_runs(64).await.unwrap();
    let refreshed = f
        .world
        .runtime
        .native
        .as_ref()
        .unwrap()
        .refresh_source(&f.world.runtime, &actor.identity, device, &f.space)
        .await
        .unwrap();
    assert!(refreshed.caught_up);
    assert_eq!(Some(refreshed.through), f.head);
    assert_eq!(
        f.world.runtime.pending_reply_runs(64).await.unwrap(),
        before_runs,
        "applying the real owner review must not discover an unrelated chat run"
    );
    let runtime = &f.world.runtime;
    let run = runtime
        .prepare_image_task(
            &actor.identity,
            device,
            &request,
            price.clone(),
            gateway(&price),
        )
        .await
        .unwrap();
    assert_eq!(
        runtime
            .prepare_image_task(
                &actor.identity,
                device,
                &request,
                price.clone(),
                gateway(&price)
            )
            .await
            .unwrap(),
        run
    );
    assert_eq!(
        runtime
            .run_image_task(&run, gateway(&price))
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    let sha256 = hash(&original_bytes);
    let model = ModelGateway::new(gateway(&price)).unwrap();
    assert_eq!(
        proof::run(runtime, &run, &model, &model).await,
        FinancialState::Settled { units: 15 }
    );
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    let output = proof::output(runtime, &run, &model).await;
    assert_eq!(output.sha256, sha256);
    assert_eq!(proof::output(runtime, &run, &model).await, output);
    assert_eq!(
        runtime
            .prepare_image_task_output(&run, gateway(&price))
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    output::cold(&mut f).await;
    assert_eq!(proof::output(&f.world.runtime, &run, &model).await, output);
    assert_eq!(
        proof::bytes(&f.world.runtime, &run).await.unwrap(),
        original_bytes
    );
    let removed = proof::remove_chunk(&f.world.runtime, &run).await;
    assert!(proof::bytes(&f.world.runtime, &run).await.is_err());
    assert!(proof::try_output(&f.world.runtime, &run, &model)
        .await
        .is_err());
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    proof::restore_chunk(&f.world.runtime, removed).await;
    assert_eq!(
        proof::bytes(&f.world.runtime, &run).await.unwrap(),
        original_bytes
    );
    assert_eq!(proof::output(&f.world.runtime, &run, &model).await, output);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.world.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 15
        }
    );
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        1
    );
    assert_eq!(
        f.world
            .count("SELECT count(*) FROM runtime_dispositions WHERE kind='settle'")
            .await,
        1
    );
    delivery::run(&mut f, &run, &output).await;
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.world.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 15
        }
    );
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        1
    );
    assert_eq!(
        f.world
            .count("SELECT count(*) FROM runtime_dispositions WHERE kind='settle'")
            .await,
        1
    );
    drop(model);
    drop(provider);
    f.finish().await;
    println!("native image journey: signed review, real MLS task, shared bill, canonical blob PUT, original relay publication and lost ACK cold recovery PASS (actual unpaid loopback HTTP gateway; production output gate closed)");
}
