use super::*;
use crate::image_tasks::{ImageTaskDefinition, ImageTaskInput};

fn input() -> ImageTaskInput {
    ImageTaskInput {
        prompt: "A quiet blue sky".into(),
        definition: ImageTaskDefinition {
            version: "image-task-v1".into(),
            price_digest: "a".repeat(64),
            size: "1024x1024".into(),
            quality: "low".into(),
            background: "opaque".into(),
            max_image_bytes: 12 * 1024 * 1024,
        },
    }
}
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![122; width as usize * height as usize * 3])
            .unwrap();
    }
    bytes
}
fn approved() -> (Fixture, String, String) {
    let mut f = Fixture::new();
    f.encrypted(
        EventBody::GrantIssued {
            grant: Grant {
                id: "image-act".into(),
                grantor: f.owner_profile.id.clone(),
                grantee: Some(f.agent_profile.id.clone()),
                scope: GrantScope::Space(SPACE.into()),
                capability: Capability::Trust(TrustLevel::Act),
                expires_at_ms: None,
            },
        },
        200,
    );
    f.message("Please create a sky image.");
    let source = f.core.engine.logs[SPACE]
        .events()
        .last()
        .unwrap()
        .client_id
        .clone();
    let (request, envelope, _) = f.core.stage_image_task(SPACE, &source, input()).unwrap();
    assert!(matches!(
        f.core.image_task(&request, crate::engine::now_ms()),
        Err(NativeError::Permission)
    ));
    let frame = ordered(&f.owner, envelope);
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    f.core.ingest(vec![frame]).unwrap();
    let hash = f.core.engine.state.requests[&request]
        .req
        .content_hash
        .clone();
    f.encrypted(
        EventBody::RequestResolved {
            request: request.clone(),
            approved: true,
            content_hash: hash.clone(),
            resolution: Some(RequestResolution {
                message: String::new(),
                created_item: None,
                ai_cost_cents: 0,
                standing_grant: None,
                tool_decision: Some(ToolDecision::ApproveOnce),
            }),
        },
        crate::engine::now_ms(),
    );
    assert_eq!(
        f.core
            .image_task(&request, crate::engine::now_ms())
            .unwrap()
            .input(),
        &input()
    );
    (f, request, hash)
}
#[test]
fn approved_image_task_produces_real_encrypted_filedoc_and_cold_native_image() {
    let (mut f, request, hash) = approved();
    let before = f.core.image().unwrap();
    let bytes = png(1024, 1024);
    let prepared = f
        .core
        .prepare_image_task(&request, &hash, &bytes, crate::engine::now_ms())
        .unwrap();
    assert_eq!(prepared.file().mime, "image/png");
    assert_eq!(prepared.file().bytes, bytes.len() as u64);
    assert!(!prepared.file().chunks.is_empty());
    assert_ne!(prepared.file().chunks[0].key, "00".repeat(32));
    let item = prepared.facts().proposal().artifact.clone();
    let envelope = prepared.envelope().clone();
    let restored = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec()).unwrap(),
        f.credential(),
    )
    .unwrap();
    assert_eq!(
        restored.engine.file_bytes(&item, None).unwrap(),
        Some(bytes)
    );
    let mut cold = restored;
    assert!(
        cold.engine.outbox_envelopes().is_empty(),
        "file offer waits for encrypted blob upload receipt"
    );
    let uploads = cold.engine.pending_uploads(100);
    assert!(!uploads.is_empty());
    // Explicit fixture acknowledgement, not a claim of real HTTP blob delivery.
    for (sha, _) in uploads {
        cold.engine.upload_done(&sha);
    }
    assert_eq!(cold.engine.outbox_envelopes(), vec![envelope.clone()]);
    let frame = ordered(&f.owner, envelope);
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    cold.ingest(vec![frame]).unwrap();
    assert_eq!(cold.engine.state.items[&item].current().number, 1);
    assert!(
        cold.image_task(&request, crate::engine::now_ms()).is_err(),
        "accepted output cannot grant another attempt"
    );
    let mut invalid = DeviceCore::restore(before, f.credential()).unwrap();
    assert!(invalid
        .prepare_image_task(&request, &hash, &png(1, 1), crate::engine::now_ms())
        .is_err());
    assert!(
        invalid.image().is_err(),
        "failed scratch workspace cannot be activated"
    );
}
#[test]
fn image_task_permission_tracks_owner_review_expiry_device_scope_and_revocation() {
    let (mut f, request, _) = approved();
    let facts = f
        .core
        .image_task(&request, crate::engine::now_ms())
        .unwrap();
    assert!(f
        .core
        .image_task(&request, facts.proposal().expires_at_ms)
        .is_err());
    let original = f.core.image().unwrap();
    f.encrypted(
        EventBody::GrantRevoked {
            grant: "image-act".into(),
        },
        crate::engine::now_ms(),
    );
    assert!(f
        .core
        .image_task(&request, crate::engine::now_ms())
        .is_err());
    let mut cold = DeviceCore::restore(original, f.credential()).unwrap();
    cold.engine.net.account.as_mut().unwrap().device = "changed-device".into();
    assert!(cold.image_task(&request, crate::engine::now_ms()).is_err());
}
#[test]
fn reply_only_trust_never_opens_an_image_review() {
    let mut f = Fixture::new();
    f.grant("reply-only", None);
    f.message("Create an image");
    let source = f.core.engine.logs[SPACE]
        .events()
        .last()
        .unwrap()
        .client_id
        .clone();
    assert!(f.core.stage_image_task(SPACE, &source, input()).is_err());
    assert!(f.core.engine.state.requests.is_empty());
}

#[test]
fn image_task_uses_sql_observed_trust_expiry_and_never_revives_an_older_grant() {
    let (mut f, request, _) = approved();
    let expiry = crate::engine::now_ms() + 30_000;
    f.encrypted(
        EventBody::GrantIssued {
            grant: Grant {
                id: "image-short".into(),
                grantor: f.owner_profile.id.clone(),
                grantee: Some(f.agent_profile.id.clone()),
                scope: GrantScope::Space(SPACE.into()),
                capability: Capability::Trust(TrustLevel::Act),
                expires_at_ms: Some(expiry),
            },
        },
        crate::engine::now_ms(),
    );
    let facts = f.core.image_task(&request, expiry - 1).unwrap();
    assert!(facts.proposal().expires_at_ms > expiry);
    assert_eq!(
        facts.expires_at_ms(),
        expiry,
        "paid admission uses earlier trust deadline"
    );
    assert!(
        f.core.image_task(&request, expiry).is_err(),
        "explicit observed clock, not host wall-clock fallback"
    );
    f.encrypted(
        EventBody::GrantRevoked {
            grant: "image-short".into(),
        },
        crate::engine::now_ms(),
    );
    assert!(f
        .core
        .engine
        .state
        .grants
        .iter()
        .any(|g| g.grant.id == "image-act" && !g.revoked));
    assert!(
        f.core
            .image_task(&request, crate::engine::now_ms())
            .is_err(),
        "revoked newest baseline cannot revive older Act"
    );
}
