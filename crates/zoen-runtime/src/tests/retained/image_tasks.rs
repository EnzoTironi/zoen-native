//! Real native review, original MLS wire, shared finance and cold FileDoc root.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use client_core::image_tasks::{ImageTaskDefinition, ImageTaskInput};
use native::testing::image_tasks as proof;
use zoen_models::{GeneratedImage, ModelGateway};
mod delivery;

fn gateway() -> GatewayConfig {
    let p = crate::tests::image_tasks::profile();
    GatewayConfig {
        base_url: "https://api.openai.com/v1".into(),
        endpoint_policy: EndpointPolicy::Https,
        model: p.model,
        credential_ref: p.credential_ref,
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
    let mut price = crate::tests::image_tasks::profile();
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
    let (request, approval_hash, envelope) = proof::stage(
        &f.world.runtime,
        &actor.identity,
        device,
        &f.space,
        &source,
        ImageTaskInput {
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
    )
    .await;
    f.append(envelope).await;
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
    f.sync(false).await.unwrap();
    let runtime = &f.world.runtime;
    let run = runtime
        .prepare_image_task(&actor.identity, device, &request, price.clone(), gateway())
        .await
        .unwrap();
    assert_eq!(
        runtime
            .prepare_image_task(&actor.identity, device, &request, price, gateway())
            .await
            .unwrap(),
        run
    );
    assert_eq!(
        runtime.run_image_task(&run, gateway()).await.unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    let task = proof::task(runtime, &run).await;
    let bytes = png();
    assert!(bytes.len() > 4 * 1024 * 1024);
    let original_bytes = bytes.clone();
    let sha256 = hash(&bytes);
    let mut result = crate::tests::image_tasks::result(&task, true);
    let mut body: serde_json::Value = serde_json::from_slice(&result.evidence.body).unwrap();
    body.as_object_mut().unwrap().remove("padding");
    body["data"] = serde_json::json!([{"b64_json":STANDARD.encode(&bytes)}]);
    result.evidence.body = serde_json::to_vec(&body).unwrap();
    result.output = Ok(GeneratedImage {
        bytes,
        mime: "image/png",
        width: 1024,
        height: 1024,
        sha256: sha256.clone(),
    });
    let adapter = crate::tests::image_tasks::Provider {
        task,
        result,
        sends: Default::default(),
        close: None,
    };
    let model = ModelGateway::new(gateway()).unwrap();
    assert_eq!(
        proof::run(runtime, &run, &model, &adapter).await,
        FinancialState::Settled { units: 15 }
    );
    assert_eq!(adapter.sends.load(Ordering::SeqCst), 1);
    let output = proof::output(runtime, &run, &model).await;
    assert_eq!(output.sha256, sha256);
    assert_eq!(proof::output(runtime, &run, &model).await, output);
    assert_eq!(
        runtime
            .prepare_image_task_output(&run, gateway())
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
    assert_eq!(adapter.sends.load(Ordering::SeqCst), 1);
    proof::restore_chunk(&f.world.runtime, removed).await;
    assert_eq!(
        proof::bytes(&f.world.runtime, &run).await.unwrap(),
        original_bytes
    );
    assert_eq!(proof::output(&f.world.runtime, &run, &model).await, output);
    assert_eq!(adapter.sends.load(Ordering::SeqCst), 1);
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
    assert_eq!(adapter.sends.load(Ordering::SeqCst), 1);
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
    f.finish().await;
    println!("native image journey: signed review, real MLS task, shared bill, canonical blob PUT, original relay publication and lost ACK cold recovery PASS (controlled adapter; production output gate closed)");
}
