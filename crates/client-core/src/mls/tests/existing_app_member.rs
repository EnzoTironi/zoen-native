//! A genuine late Agent Welcome is not an App-state bootstrap.
//! No service, provider, retained account, history import or synthetic Item state.
//! Diagnostic RED until an explicit canonical current-state share is inserted;
//! a save must not silently disclose old history as its repair.
use super::*;
use base64::Engine as _;
use miniapps::{binary::Format, wordcraft};
use std::collections::HashMap;
use types::ItemContent;

mod controls;
struct LateWord {
    owner: Engine,
    agent: Engine,
    blobs: HashMap<String, Vec<u8>>,
    original: crate::AppBinaryDocumentDto,
    original_root: crate::TimelineEntry,
    owner_secret: [u8; 32],
    agent_secret: [u8; 32],
}

const SPACE: &str = "existing-word-new-agent";
const EDITED: &[u8] =
    include_bytes!("../../../../miniapps/src/wordcraft/fixtures/actual-wasi.docx");

fn publish(
    owner: &mut Engine,
    agent: &mut Engine,
    blobs: &mut HashMap<String, Vec<u8>>,
) -> Vec<Sequenced> {
    blobs.extend(owner.pending_uploads(100));
    relay_ready_to_peer(owner, agent)
}

fn receive_wanted(agent: &mut Engine, blobs: &HashMap<String, Vec<u8>>) -> R<()> {
    for wanted in agent.wanted_media() {
        let sealed = blobs
            .get(&wanted.blob)
            .ok_or_else(|| storage("fixture lacks the actually requested ciphertext"))?;
        assert!(agent.media_arrived(&wanted, sealed));
    }
    Ok(())
}

fn late_word(owner_path: &str, agent_path: &str) -> R<LateWord> {
    late_word_mode(owner_path, agent_path, false)
}
fn late_word_mode(owner_path: &str, agent_path: &str, legacy: bool) -> R<LateWord> {
    let (mut owner, author, owner_secret) = member(owner_path);
    let mut agent = Engine::open(agent_path)?;
    let root = [93; 32];
    let agent_id = Signer::from_secret(&root).id();
    let proof = event_log::agent_owner::authorize(&author, &agent_id)
        .ok_or_else(|| storage("fixture needs a certified owner"))?;
    let (_, agent_secret, _) = agent.create_agent_account(
        "Word collaborator",
        "word_collaborator",
        "http://relay.test",
        root,
        &author.identity,
        proof,
    )?;
    let profile = agent
        .identities
        .get(&agent_id)
        .cloned()
        .ok_or_else(|| storage("fixture needs the genuine Agent profile"))?;
    assert!(event_log::agent_owner::profile_authorized(&profile));
    owner.store.put_identity(&profile, None)?;
    owner.identities.insert(agent_id.clone(), profile);
    owner.identity_order.push(agent_id.clone());
    // Public directory metadata only; no owner key/history is imported.
    let profile = owner
        .identities
        .get(&author.identity)
        .cloned()
        .ok_or_else(|| storage("fixture owner profile"))?;
    agent.store.put_identity(&profile, None)?;
    agent.identities.insert(profile.id.clone(), profile.clone());
    agent.identity_order.push(profile.id);
    for frame in claim_group(&mut owner, SPACE, &mut []) {
        assert_eq!(agent.ingest(frame), Ingest::Applied);
    }
    let mut blobs = HashMap::new();
    let created = owner.app_binary_create_format(
        Format::Wordcraft,
        SPACE,
        "Retained Word",
        &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
        "before_join",
    )?;
    if legacy {
        // Reproduce the old canonical producer bytes with a genuine signature
        // before sealing/publication; no Item or MLS state is fabricated.
        let row = owner
            .store
            .outbox()?
            .into_iter()
            .find(|r| {
                matches!(&r.event.body,
            EventBody::ItemCreated { item, .. } if item == &created.item_id)
            })
            .ok_or_else(|| storage("original App creation intent"))?;
        let mut body = row.event.body.clone();
        if let EventBody::ItemCreated {
            content: ItemContent::App(app),
            ..
        } = &mut body
        {
            app.document
                .as_mut()
                .ok_or_else(|| storage("owned document"))?
                .app_origin = None;
        }
        let old = author.sign_event(
            SPACE,
            &row.event.client_id,
            row.event.at_ms,
            row.event.seen.clone(),
            body,
        );
        // outbox_put is intentionally insert-only. Install these re-signed old
        // producer bytes in this private, not-yet-published row without changing
        // its FIFO position or any canonical/MLS state.
        owner
            .store
            .conn()
            .execute(
                "UPDATE outbox SET json=?1 WHERE client_id=?2",
                rusqlite::params![
                    serde_json::to_string(&old).map_err(|e| storage(&e.to_string()))?,
                    old.client_id
                ],
            )
            .map_err(|e| storage(&e.to_string()))?;
        owner.reproject_approval_overlay()?;
    }
    let original_root = owner
        .item_discussion(SPACE, &created.item_id)?
        .root
        .ok_or_else(|| storage("fixture needs the original canonical card"))?;
    let original = owner.app_binary_save(
        &created.item_id,
        created.version,
        &created.sha256,
        &base64::engine::general_purpose::STANDARD.encode(EDITED),
        "real_pre_join_save",
        &event_log::sha256_hex(b"real pre-join Word save"),
    )?;
    assert_eq!(original.version, 2);
    let before_join = publish(&mut owner, &mut agent, &mut blobs);
    assert!(before_join
        .iter()
        .any(|frame| { frame.env.sealed_kind() == Some(SealedKind::Application) }));
    assert!(agent.item_state(&original.item_id).is_err());

    // Normal signed MemberAdded followed by a genuine certified-device MLS
    // Commit/Welcome; the old application ciphertext is never imported/opened.
    let agent_author = agent
        .net
        .author
        .clone()
        .ok_or_else(|| storage("fixture needs the Agent device author"))?;
    add_member(&mut owner, &author, SPACE, &agent_author);
    let added = owner.logs[SPACE]
        .events()
        .last()
        .ok_or_else(|| storage("fixture needs MemberAdded"))?;
    let frame = ordered(&agent, Envelope::plain(added));
    assert_eq!(agent.ingest(frame), Ingest::Applied);
    let records = claimed_records(
        std::slice::from_mut(&mut agent),
        std::slice::from_ref(&agent_id),
    );
    let package = &records
        .first()
        .ok_or_else(|| storage("fixture needs the claimed Agent KeyPackage"))?
        .data;
    owner.commit_now(SPACE, std::slice::from_ref(package), &Default::default())?;
    publish(&mut owner, &mut agent, &mut blobs);
    assert_eq!(owner.mls_status(SPACE), agent.mls_status(SPACE));
    assert!(owner.ready_epoch(SPACE).is_some());
    assert!(agent.ready_epoch(SPACE).is_some());
    assert!(agent.item_state(&original.item_id).is_err());

    // Positive control: a new Word created only AFTER the same join is read
    // through real encrypted FileDoc chunks and the existing App reader.
    let control = owner.app_binary_create_format(
        Format::Wordcraft,
        SPACE,
        "Post-join control",
        &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
        "after_join",
    )?;
    publish(&mut owner, &mut agent, &mut blobs);
    receive_wanted(&mut agent, &blobs)?;
    let readable = agent.app_binary_read(&control.item_id, Some(1), Some(&control.sha256))?;
    assert_eq!(readable.item_id, control.item_id);
    assert_eq!(readable.version, 1);
    assert_eq!(readable.sha256, control.sha256);
    assert_eq!(readable.data_base64, control.data_base64);
    assert!(agent
        .item_discussion(SPACE, &control.item_id)?
        .root
        .is_some());

    Ok(LateWord {
        owner,
        agent,
        blobs,
        original,
        original_root,
        owner_secret,
        agent_secret,
    })
}

#[test]
fn late_certified_agent_existing_word_requires_current_state_bootstrap() -> R<()> {
    let LateWord {
        mut owner,
        mut agent,
        mut blobs,
        original,
        original_root,
        ..
    } = late_word(":memory:", ":memory:")?;
    let agent_id = agent.me_id()?;
    let account = owner.account().ok_or_else(|| storage("owner account"))?;
    let context = owner.app_current_share_context(crate::app_current_share::AppShareScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: SPACE.into(),
        item_id: original.item_id.clone(),
        target_id: agent_id,
    })?;
    let receipt = owner.share_app_current(context, "explicit_current_share")?;
    assert!(receipt.applied);
    publish(&mut owner, &mut agent, &mut blobs);
    receive_wanted(&mut agent, &blobs)?;
    let shared_v2 = agent.app_binary_read(&original.item_id, Some(2), Some(&original.sha256))?;
    assert_eq!(shared_v2.data_base64, original.data_base64);
    assert!(agent
        .app_binary_read(&original.item_id, Some(1), None)
        .is_err());
    assert_eq!(
        agent
            .item_state(&original.item_id)?
            .versions
            .iter()
            .map(|v| v.number)
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert!(agent.item_state(&original.item_id)?.shared.is_some());
    assert_eq!(
        agent.trust(&agent.me_id()?, SPACE),
        types::TrustLevel::Suggest
    );
    // Only the human-explicit sharing operation opens the old Item. The normal
    // next save then extends the same actual v2 to v3, without a fabricated v1.
    let updated = owner.app_binary_save(
        &original.item_id,
        original.version,
        &original.sha256,
        &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
        "real_post_join_save",
        &event_log::sha256_hex(b"real post-join Word save"),
    )?;
    assert_eq!(updated.version, 3);
    let saved = publish(&mut owner, &mut agent, &mut blobs);
    let decoded = saved.iter().any(|frame| {
        agent.logs[SPACE].events().iter().any(|event| {
            event.hash == frame.hash
                && matches!(&event.body,
                    EventBody::ItemVersioned { item, content: ItemContent::App(app), .. }
                    if item == &original.item_id
                        && app.document.as_ref().is_some_and(|file| file.sha256 == updated.sha256))
        })
    });
    assert!(
        decoded,
        "new save must actually decrypt before testing projection"
    );
    receive_wanted(&mut agent, &blobs)?;

    // Desired same-document behavior. Current State::apply_checked discards this
    // readable ItemVersioned because the original ItemCreated stayed sealed.
    let shared = agent.app_binary_read(
        &original.item_id,
        Some(updated.version),
        Some(&updated.sha256),
    );
    assert!(
        shared.is_ok(),
        "decrypted current save lacks Item bootstrap: {shared:?}"
    );
    let shared = shared?;
    assert_eq!(shared.item_id, original.item_id);
    assert_eq!(shared.version, updated.version);
    assert_eq!(shared.sha256, updated.sha256);
    assert_eq!(shared.data_base64, updated.data_base64);
    assert_eq!(
        agent
            .item_discussion(SPACE, &original.item_id)?
            .root
            .map(|root| root.id),
        Some(original_root.id),
    );
    Ok(())
}
