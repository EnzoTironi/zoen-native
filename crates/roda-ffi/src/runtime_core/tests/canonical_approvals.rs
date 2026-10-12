use super::*;
use crate::pages::PlanOrigin;
use roda_docs::plan::{CanonicalPlanSeed, LegacyVersion};
use roda_log::{content_hash, content_hash_of};
use sha2::{Digest, Sha256};

fn digest_field(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

#[test]
fn missing_canonical_bytes_block_both_native_authority_consumers_until_repaired() {
    let mut fixture = Fixture::new();
    let item = "native-confirmed-plan";
    let document = PlanDoc {
        title: "Native linked approval".into(),
        summary: String::new(),
        budget_cents: None,
        sections: vec![PlanSection {
            title: "Section".into(),
            lines: vec![PlanLine {
                id: "line".into(),
                text: "Exact native row".into(),
                cost_cents: 27,
                done: false,
            }],
        }],
    };
    let creation_hash = fixture.clear(EventBody::ItemCreated {
        item: item.into(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(document.clone()),
        origin: "Explicit owner creation".into(),
    });
    let creation = fixture.core.engine.logs[SPACE]
        .events()
        .iter()
        .find(|event| event.hash == creation_hash)
        .unwrap();
    let PlanOrigin::Explicit(origin) = &fixture.core.engine.state.plan_origins[item] else {
        panic!("authenticated explicit creation required")
    };
    let mut digest = Sha256::new();
    digest.update(b"zoen.plan.legacy.prefix.v1\0");
    for bytes in [
        SPACE.as_bytes(),
        item.as_bytes(),
        origin.as_bytes(),
        creation.content.as_slice(),
    ] {
        digest_field(&mut digest, bytes);
    }
    let manifest = PlanManifestV1 {
        schema: "plan_v1".into(),
        space: SPACE.into(),
        item: item.into(),
        creation_event: origin.clone(),
        legacy_versions: 1,
        legacy_head: origin.clone(),
        prefix_sha256: hex::encode(digest.finalize()),
    };
    let canonical = CanonicalPlanSeed::replay(&[LegacyVersion {
        event_id: origin.clone(),
        document: document.clone(),
    }])
    .unwrap()
    .open_editor()
    .unwrap();
    let bytes = canonical.full_update().to_json().unwrap();
    let sha = hex::encode(Sha256::digest(&bytes));
    assert!(fixture
        .owner
        .import_media(&sha, &bytes, "application/json")
        .unwrap());
    assert!(fixture
        .core
        .engine
        .import_media(&sha, &bytes, "application/json")
        .unwrap());
    fixture.clear(EventBody::ItemVersioned {
        item: item.into(),
        content: ItemContent::Page(PageContent {
            title: document.title.clone(),
            path: String::new(),
            loro: String::new(),
            manifest: Some(PageManifest::PlanV1(manifest)),
            blob: Some(ChunkRef {
                sha256: sha.clone(),
                bytes: bytes.len() as u64,
                blob: "11".repeat(32),
                key: "22".repeat(32),
            }),
        }),
        note: "Canonical confirmed version".into(),
    });
    let line = &document.sections[0].lines[0];
    let request = AgentRequest {
        id: "native-linked-request".into(),
        agent: fixture.agent_profile.id.clone(),
        title: "Owner decision".into(),
        detail: String::new(),
        audience: SPACE.into(),
        action: ActionClass::Money { cents: 27 },
        content_hash: content_hash(&(&line.text, line.cost_cents)),
        item: Some(item.into()),
        line: Some(line.id.clone()),
        proposal: None,
    };
    let author = fixture.core.engine.net.author.as_ref().unwrap();
    let event = author.sign_event(
        SPACE,
        &new_id("agent-request"),
        150,
        fixture.core.engine.logs[SPACE].head(),
        EventBody::RequestOpened {
            request: request.clone(),
        },
    );
    let frame = ordered(&fixture.owner, Envelope::plain(&event));
    assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
    fixture.core.ingest(vec![frame]).unwrap();
    fixture.clear(EventBody::RequestResolved {
        request: request.id.clone(),
        approved: true,
        content_hash: request.content_hash.clone(),
        resolution: None,
    });
    assert_eq!(
        fixture.core.engine.state.requests[&request.id].status,
        crate::engine::ReqStatus::Approved
    );
    fixture.grant("native-after-approved-plan", Some(1000));
    let trigger = fixture.message("Certified owner message after the decision");
    let facts = fixture.core.reply(SPACE, &trigger, 400).unwrap();
    let original = facts.intent().clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Retained exact reply", 500)
        .unwrap();
    let client_id = prepared.envelope().client_id().to_owned();
    assert!(fixture.core.reply_reference(&client_id).unwrap().is_some());
    let grant_digest = original.grant_digest.clone();
    let trigger_digest = content_hash_of(
        fixture.core.engine.logs[SPACE]
            .events()
            .iter()
            .find(|event| event.hash == trigger)
            .unwrap(),
    );

    for (reader, at_ms) in [(false, 1), (true, 1_900_000_000_000)] {
        let other = new_id("native-unrelated-space");
        let owner_author = fixture.owner.net.author.clone().unwrap();
        let agent_author = fixture.core.engine.net.author.clone().unwrap();
        let mut foreign_document = document.clone();
        foreign_document.title = "Unrelated scoped content".into();
        foreign_document.sections[0].lines[0].text = "Different authenticated row".into();
        foreign_document.sections[0].lines[0].cost_cents = 987;
        let line = &foreign_document.sections[0].lines[0];
        let mut foreign_request = request.clone();
        foreign_request.title = "Unrelated duplicate request".into();
        foreign_request.content_hash = content_hash(&(&line.text, line.cost_cents));
        let mut bodies = vec![
            EventBody::SpaceCreated {
                title: "Unrelated retained history".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Public,
            },
            EventBody::MemberAdded {
                identity: fixture.agent_profile.id.clone(),
                role: Role::Member,
            },
        ];
        if reader {
            bodies.push(EventBody::MemberAdded {
                identity: fixture.owner_profile.id.clone(),
                role: Role::Reader,
            });
        }
        bodies.extend([
            EventBody::ItemCreated {
                item: item.into(),
                kind: ItemKind::Plan,
                content: ItemContent::Plan(foreign_document),
                origin: "Different Space origin".into(),
            },
            EventBody::RequestOpened {
                request: foreign_request.clone(),
            },
            EventBody::RequestResolved {
                request: request.id.clone(),
                approved: false,
                content_hash: foreign_request.content_hash.clone(),
                resolution: None,
            },
        ]);
        for body in bodies {
            let author = if matches!(&body, EventBody::RequestOpened { .. }) {
                &agent_author
            } else {
                &owner_author
            };
            let event = author.sign_event(
                &other,
                &new_id("native-unrelated-event"),
                at_ms,
                fixture.owner.logs.get(&other).and_then(SpaceLog::head),
                body,
            );
            let frame = ordered(&fixture.owner, Envelope::plain(&event));
            assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
            fixture.core.ingest(vec![frame]).unwrap();
        }
        assert_eq!(
            fixture.core.engine.space_projections[SPACE].requests[&request.id].status,
            crate::engine::ReqStatus::Approved
        );
        assert!(!fixture.core.engine.state.requests.contains_key(&request.id));
        let current = fixture.core.reply(SPACE, &trigger, 400).unwrap();
        assert_eq!(current.intent(), &original);
        assert!(fixture.core.reply_reference(&client_id).unwrap().is_some());
        let cold =
            DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()).unwrap();
        assert_eq!(
            cold.engine.space_projections[SPACE].requests[&request.id].status,
            crate::engine::ReqStatus::Approved
        );
        assert_eq!(
            cold.reply(SPACE, &trigger, 400).unwrap().intent(),
            &original
        );
        assert!(cold.reply_reference(&client_id).unwrap().is_some());
    }

    fixture
        .core
        .engine
        .store
        .conn()
        .execute("DELETE FROM media WHERE sha256 = ?1", [&sha])
        .unwrap();
    assert!(matches!(
        fixture.core.reply(SPACE, &trigger, 400),
        Err(NativeError::History)
    ));
    assert!(matches!(
        fixture.core.reply_reference(&client_id),
        Err(NativeError::History)
    ));
    assert!(fixture
        .core
        .engine
        .import_media(&sha, &bytes, "application/json")
        .unwrap());
    let recovered = fixture.core.reply(SPACE, &trigger, 400).unwrap();
    assert_eq!(recovered.intent(), &original);
    assert_eq!(recovered.intent().grant_digest, grant_digest);
    assert_eq!(recovered.intent().trigger_digest, trigger_digest);
    assert!(fixture.core.reply_reference(&client_id).unwrap().is_some());
}
