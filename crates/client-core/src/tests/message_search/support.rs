//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn message_hits(engine: &Client, term: &str) -> Vec<UniversalHit> {
    engine
        .universal_search(term.into(), vec!["message".into()], 100)
        .unwrap()
        .hits
}

pub(super) fn activity_account(path: &str) -> (Arc<Client>, Arc<MemVault>) {
    let engine = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(MemVault::default());
    engine
        .create_account(
            "Activity author".into(),
            "activity_author".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    (engine, vault)
}

pub(super) fn activity_space(engine: &Client, title: &str) -> String {
    engine
        .create_group_with(title.into(), vec![], PrivacyDto::Closed)
        .unwrap()
}

pub(super) fn check_cilantro(engine: &Client, space: &str, recipe: &str) -> TimelineEntry {
    let result = engine
        .app_call_tool(
            recipe.into(),
            "recipe_check".into(),
            r#"{"id":"i7"}"#.into(),
            false,
        )
        .unwrap();
    assert_eq!(result.status, AppCallStatus::Done);
    engine
        .timeline(space.into())
        .unwrap()
        .into_iter()
        .rev()
        .find(|entry| matches!(&entry.kind, EntryKind::ItemEdited { item_id, note, .. } if item_id == recipe && note.contains("Cilantro")))
        .expect("the real recipe action publishes its visible ingredient note")
}
