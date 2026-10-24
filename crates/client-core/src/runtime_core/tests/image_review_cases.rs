use super::*;

pub(super) fn pending(
) -> std::result::Result<(Fixture, String, String, Envelope), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.encrypted(
        EventBody::GrantIssued {
            grant: Grant {
                id: "image-review-act".into(),
                grantor: fixture.owner_profile.id.clone(),
                grantee: Some(fixture.agent_profile.id.clone()),
                scope: GrantScope::Space(SPACE.into()),
                capability: Capability::Trust(TrustLevel::Act),
                expires_at_ms: None,
            },
        },
        200,
    );
    fixture.message("Create an image after I review it.")?;
    let source = fixture.core.engine.logs[SPACE]
        .events()
        .last()
        .unwrap()
        .client_id
        .clone();
    let (review, envelope, _) = fixture
        .core
        .stage_image_task(SPACE, &source, image_task_cases::input())
        .unwrap();
    Ok((
        fixture,
        review.id,
        review.tool_approval.unwrap().content_hash,
        envelope,
    ))
}

#[test]
fn only_the_original_current_pending_review_can_be_offered_after_cold_reopen() {
    let (mut fixture, request, content_hash, envelope) = pending().unwrap();
    let now = crate::engine::now_ms();
    fixture
        .core
        .image_review_ready(&request, &content_hash, &envelope, now)
        .unwrap();
    let before = fixture.core.image().unwrap();
    assert!(fixture
        .core
        .image_review_ready(&request, &"a".repeat(64), &envelope, now)
        .is_err());
    let mut different = envelope.clone();
    different.sig = "a".repeat(128);
    assert!(fixture
        .core
        .image_review_ready(&request, &content_hash, &different, now)
        .is_err());
    assert!(fixture
        .core
        .image_review_ready(&request, &content_hash, &envelope, now + 16 * 60 * 1000)
        .is_err());
    assert_eq!(fixture.core.image().unwrap().as_bytes(), before.as_bytes());
    fixture.core = DeviceCore::restore(before, fixture.credential()).unwrap();
    fixture
        .core
        .image_review_ready(&request, &content_hash, &envelope, now)
        .unwrap();
    let frame = ordered(&fixture.owner, envelope.clone());
    assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
    fixture.core.ingest(vec![frame]).unwrap();
    assert!(matches!(
        fixture
            .core
            .image_review_ready(&request, &content_hash, &envelope, now),
        Err(NativeError::NotReady)
    ));
}

#[test]
fn fresh_revocation_refuses_review_publication_without_clearing_its_original() {
    let (mut fixture, request, content_hash, envelope) = pending().unwrap();
    fixture.encrypted(
        EventBody::GrantRevoked {
            grant: "image-review-act".into(),
        },
        crate::engine::now_ms(),
    );
    let queued = fixture.core.engine.store.outbox().unwrap();
    assert!(fixture
        .core
        .image_review_ready(&request, &content_hash, &envelope, crate::engine::now_ms())
        .is_err());
    assert_eq!(
        fixture.core.engine.store.outbox().unwrap().len(),
        queued.len()
    );
    assert_eq!(
        fixture
            .core
            .engine
            .store
            .outbox_get(envelope.client_id())
            .unwrap(),
        queued
            .into_iter()
            .find(|row| row.event.client_id == envelope.client_id())
            .map(|row| row.event)
    );
}
