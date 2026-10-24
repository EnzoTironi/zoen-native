//! Actual signed source and normal discovery; no worker/provider/services needed.
use super::*;
use client_core::SecretVault;
use sha2::{Digest, Sha256};

#[test]
fn scoped_discovery_and_dispatch_never_fall_back_to_other_space_tools_or_cached_calls() {
    let f = crate::deckcraft::tests::fixture();
    let space = f.client.item(f.base.item_id.clone()).unwrap().space_id;
    let page = f
        .client
        .page_import_markdown(space.clone(), "outside.md".into(), "# Sibling\n".into())
        .unwrap();
    let journal = f.path.join("scope-steps.json");
    let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let context = page_tool_tests::cx(&f.client, &journal, &page.id, starts);
    let legacy = SpaceTools::new(&f.client, &context, &space, &f.owner, &f.source);
    assert!(legacy.tools().iter().any(
        |d| d.name == client_core::agent_page_tools::model_name("read_page", &page.id).unwrap()
    ));
    drop(legacy);
    drop(context);
    let root = f
        .client
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(
            |e| matches!(&e.kind,EntryKind::Message{card:Some(card),..} if card.id==f.base.item_id),
        )
        .unwrap()
        .id;
    let key: [u8; 32] = f
        .owner_vault
        .load("zoen.identity.v1".into())
        .unwrap()
        .try_into()
        .unwrap();
    let vault = f.vault.clone();
    let path = f.path.clone();
    let owner = f.owner.clone();
    let base = f.base.clone();
    drop(f);
    let db = store::Store::open(path.join("agent.sqlite").to_str().unwrap()).unwrap();
    let mut log = event_log::SpaceLog::from_events(&space, db.events(&space).unwrap()).unwrap();
    let existing: BTreeSet<_> = log.events().iter().map(|e| e.client_id.clone()).collect();
    for pending in db
        .outbox()
        .unwrap()
        .into_iter()
        .filter(|p| p.event.space == space)
    {
        if !existing.contains(&pending.event.client_id) {
            db.append_event(log.sequence(pending.event.clone()))
                .unwrap();
        }
        db.outbox_remove(&pending.event.client_id).unwrap();
    }
    let text = "Only edit this App";
    let mut reply = types::ReplyRef::thread(root);
    reply.artifact = Some(types::ArtifactRequest {
        item_id: base.item_id.clone(),
        version: base.version,
        sha256: base.sha256.clone(),
        request_id: types::new_id("app_request"),
        text_sha256: hex::encode(Sha256::digest(text.as_bytes())),
    });
    let source = types::new_id("source");
    let event = log.append(
        &event_log::Signer::from_secret(&key),
        1_792_510_810_000,
        types::EventBody::AppEditRequested {
            message: source.clone(),
            text: text.into(),
            reply,
        },
    );
    db.append_event(event).unwrap();
    log.verify().unwrap();
    drop(db);
    let client = canonical_tool_approval_tests::reopen(&path, vault);
    let context = page_tool_tests::cx(
        &client,
        &journal,
        &page.id,
        Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    );
    let scoped = SpaceTools::new(&client, &context, &space, &owner, &source);
    // No binary worker configured, so the scoped source offers zero tools; the
    // sibling Page remains available to legacy sources only.
    assert!(scoped.tools().is_empty());
    let call = ToolCall {
        id: "cached-outside".into(),
        name: client_core::agent_page_tools::model_name("read_page", &page.id).unwrap(),
        arguments: json!({"artifactId":page.id}),
        requires_review: false,
    };
    scoped.record(ToolStep {
        call: call.clone(),
        result: "lateral cached data".into(),
        is_error: false,
    });
    let before = std::fs::read(&journal).unwrap();
    assert!(scoped.call(&call).is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    assert!(client
        .agent_artifact_request(&space, &source, &owner)
        .unwrap()
        .is_some());
    drop(scoped);
    drop(context);
    drop(client);
    std::fs::remove_dir_all(path).unwrap();
}
