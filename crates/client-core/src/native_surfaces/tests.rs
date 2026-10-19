use super::*;
use crate::{PrivacyDto, SecretVault};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}

struct Fixture {
    engine: Arc<Client>,
    identity: String,
    device: String,
    space: String,
}
impl Fixture {
    fn new() -> Self {
        let engine = Client::open(":memory:".into(), "en".into()).unwrap();
        engine
            .create_account(
                "Surface author".into(),
                "surface_author".into(),
                "http://127.0.0.1:1".into(),
                Arc::new(Vault::default()),
            )
            .unwrap();
        let space = engine
            .create_group_with("Surfaces".into(), vec![], PrivacyDto::Closed)
            .unwrap();
        let account = engine.account().unwrap();
        Self {
            engine,
            identity: account.identity_id,
            device: account.device_id,
            space,
        }
    }
    fn list(&self, title: &str) -> String {
        self.engine
            .install_app(
                self.space.clone(),
                "list".into(),
                json!({"title":title,"items":[]}).to_string(),
            )
            .unwrap()
            .id
    }
    fn read(&self, ids: Vec<String>) -> R<Vec<NativeSurfaceItemDto>> {
        self.engine
            .native_surface_items(self.identity.clone(), self.device.clone(), ids)
    }
    fn counts(&self) -> (u64, usize) {
        let core = self.engine.lock();
        (
            core.store.event_count().unwrap(),
            core.store.outbox().unwrap().len(),
        )
    }
}

#[test]
fn batch_preserves_requested_order_deduplicates_and_has_only_compact_fields() {
    let f = Fixture::new();
    let a = f.list("First");
    let b = f.list("Second");
    let before = f.counts();
    let rows = f
        .read(vec![
            b.clone(),
            a.clone(),
            b.clone(),
            "it_missing".into(),
            a.clone(),
        ])
        .unwrap();
    assert_eq!(rows.iter().map(|r| &r.id).collect::<Vec<_>>(), vec![&b, &a]);
    assert_eq!(f.read(vec![a.clone(); 64]).unwrap().len(), 1);
    let wire = serde_json::to_value(&rows[0]).unwrap();
    let mut fields = wire
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    fields.sort_unstable();
    assert_eq!(fields, ["app", "id", "space_id", "title", "version"]);
    let app = wire["app"].as_object().unwrap();
    assert_eq!(app.len(), 2);
    assert!(app.contains_key("app_id") && app.contains_key("snapshot_json"));
    let snapshot: Value = serde_json::from_str(&rows[0].app.snapshot_json).unwrap();
    assert_eq!(snapshot["id"], b);
    assert_eq!(snapshot["appId"], "list");
    assert_eq!(snapshot["deepLink"], format!("zoen://app/{}", rows[0].id));
    assert_eq!(
        f.counts(),
        before,
        "projection must not append, enqueue or save preferences"
    );
}

#[test]
fn whole_request_bounds_and_ascii_syntax_are_checked_before_owner_or_item_work() {
    let engine = Client::open(":memory:".into(), "en".into()).unwrap();
    for ids in [
        vec!["it_x".into(); 65],
        vec!["".into()],
        vec!["it/x".into()],
        vec!["it_é".into()],
        vec!["a".repeat(129)],
        vec!["it_ok".into(), "it_bad\n".into()],
    ] {
        assert!(matches!(
            engine.native_surface_items("owner".into(), "device".into(), ids),
            Err(CoreError::Invalid { .. })
        ));
    }
    for owner in ["".to_string(), "owner/other".into(), "o".repeat(257)] {
        assert!(matches!(
            engine.native_surface_items(owner, "device".into(), vec![]),
            Err(CoreError::Invalid { .. })
        ));
    }
    assert!(
        matches!(
            engine.native_surface_items("owner".into(), "device".into(), vec![]),
            Err(CoreError::Forbidden { .. })
        ),
        "an empty batch still requires the current unlocked owner"
    );
}

#[test]
fn wrong_owner_device_locked_setup_or_revoked_session_cannot_publish() {
    let f = Fixture::new();
    let item = f.list("Private");
    assert!(f
        .engine
        .native_surface_items("other_owner".into(), f.device.clone(), vec![item.clone()])
        .is_err());
    assert!(f
        .engine
        .native_surface_items(
            f.identity.clone(),
            "other_device".into(),
            vec![item.clone()]
        )
        .is_err());
    let author = f.engine.lock().net.author.take().unwrap();
    assert!(f.read(vec![item.clone()]).is_err());
    f.engine.lock().net.author = Some(author);
    f.engine
        .lock()
        .store
        .set_meta("provider_signup_pending", "true")
        .unwrap();
    assert!(f.read(vec![item.clone()]).is_err());
    f.engine
        .lock()
        .store
        .meta_delete("provider_signup_pending")
        .unwrap();
    f.engine
        .lock()
        .store
        .set_meta("devices.revoked", &json!([f.device]).to_string())
        .unwrap();
    assert!(f.read(vec![item.clone()]).is_err());
    f.engine
        .lock()
        .store
        .set_meta("devices.revoked", "not JSON")
        .unwrap();
    assert!(
        matches!(f.read(vec![item]), Err(CoreError::Storage { .. })),
        "malformed revocation metadata must not grant access"
    );
}

#[test]
fn signed_device_binding_is_required_even_when_account_metadata_matches() {
    let f = Fixture::new();
    let item = f.list("Private");
    let mut core = f.engine.lock();
    core.net.author.as_mut().unwrap().cert = Some("bad".into());
    core.net.account.as_mut().unwrap().cert = "bad".into();
    drop(core);
    assert!(
        f.read(vec![item]).is_err(),
        "matching malformed certificates are not an unlocked certified device"
    );
}

#[test]
fn reads_current_v2_using_the_canonical_snapshot_without_changing_state() {
    let f = Fixture::new();
    let item = f.list("Current list");
    f.engine
        .app_call_tool(
            item.clone(),
            "list_add".into(),
            json!({"text":"Fresh canonical row"}).to_string(),
            false,
        )
        .unwrap();
    let full = f.engine.item(item.clone()).unwrap();
    assert_eq!(full.version, 2);
    let before = f.counts();
    let projected = f.read(vec![item.clone()]).unwrap().pop().unwrap();
    assert_eq!(projected.version, 2);
    assert_eq!(projected.title, full.title);
    assert_eq!(projected.app.snapshot_json, full.app.unwrap().snapshot_json);
    let snap: Value = serde_json::from_str(&projected.app.snapshot_json).unwrap();
    assert_eq!(snap["rows"][0]["text"], "Fresh canonical row");
    assert_eq!(f.engine.item(item).unwrap().version, 2);
    assert_eq!(f.counts(), before);
}

#[test]
fn current_space_membership_and_projection_override_a_stale_merged_index() {
    let f = Fixture::new();
    let item = f.list("Scoped");
    // A stale merged membership record must not authorize a Space read.
    f.engine
        .lock()
        .space_projections
        .get_mut(&f.space)
        .unwrap()
        .spaces
        .get_mut(&f.space)
        .unwrap()
        .members
        .clear();
    assert!(f.read(vec![item.clone()]).unwrap().is_empty());
    assert!(
        f.engine.lock().is_member(&f.space, &f.identity),
        "the test retains a stale merged membership"
    );
    // Restore the projection, then perform a real membership removal through the engine.
    let members = f.engine.lock().state.spaces[&f.space].members.clone();
    f.engine
        .lock()
        .space_projections
        .get_mut(&f.space)
        .unwrap()
        .spaces
        .get_mut(&f.space)
        .unwrap()
        .members = members;
    assert_eq!(f.read(vec![item.clone()]).unwrap().len(), 1);
    f.engine.leave_space(f.space.clone()).unwrap();
    assert!(f.read(vec![item]).unwrap().is_empty());
}

#[test]
fn missing_removed_non_app_and_broken_space_items_clear_publications() {
    let f = Fixture::new();
    let removed = f.list("Removed");
    let non_app = f
        .engine
        .page_create(f.space.clone(), "A page".into())
        .unwrap()
        .id;
    let good = f.list("Good");
    // No tombstoned item exists in the canonical current projection; do not revive
    // a removed item from a still-readable merged cache or its version history.
    f.engine
        .lock()
        .space_projections
        .get_mut(&f.space)
        .unwrap()
        .items
        .remove(&removed);
    assert_eq!(
        f.read(vec![removed, non_app, "it_missing".into(), good.clone()])
            .unwrap()
            .iter()
            .map(|v| &v.id)
            .collect::<Vec<_>>(),
        vec![&good]
    );
    let broken = f.engine.lock().state.spaces[&f.space].clone();
    f.engine
        .lock()
        .broken_spaces
        .insert(f.space.clone(), broken);
    assert!(f.read(vec![good]).unwrap().is_empty());
}

#[test]
fn malformed_unknown_empty_or_oversized_snapshots_are_omitted_without_truncation() {
    let f = Fixture::new();
    let item = f.list("Bounded");
    let baseline = f.engine.lock().space_projections[&f.space].items[&item].clone();
    for state in [
        "{".into(),
        "null".into(),
        "[]".into(),
        json!({"title":"","items":[]}).to_string(),
        json!({"title":"x".repeat(MAX_SNAPSHOT_BYTES),"items":[]}).to_string(),
    ] {
        let mut changed = baseline.clone();
        let ItemContent::App(doc) = &mut changed.versions.last_mut().unwrap().content else {
            unreachable!()
        };
        doc.state_json = state;
        f.engine
            .lock()
            .space_projections
            .get_mut(&f.space)
            .unwrap()
            .items
            .insert(item.clone(), changed);
        assert!(f.read(vec![item.clone()]).unwrap().is_empty());
    }
    let mut unknown = baseline.clone();
    let ItemContent::App(doc) = &mut unknown.versions.last_mut().unwrap().content else {
        unreachable!()
    };
    doc.app = "not-a-supported-app".into();
    f.engine
        .lock()
        .space_projections
        .get_mut(&f.space)
        .unwrap()
        .items
        .insert(item.clone(), unknown);
    assert!(f.read(vec![item.clone()]).unwrap().is_empty());
    f.engine
        .lock()
        .space_projections
        .get_mut(&f.space)
        .unwrap()
        .items
        .insert(item.clone(), baseline);
    let snapshot = f.read(vec![item]).unwrap().pop().unwrap().app.snapshot_json;
    assert!(snapshot.len() <= MAX_SNAPSHOT_BYTES);
    assert!(serde_json::from_str::<Value>(&snapshot).is_ok());
}

#[tokio::test]
async fn json_command_has_the_same_owner_bound_compact_contract() {
    let f = Fixture::new();
    let item = f.list("JSON");
    let request = json!({"op":"native_surface_items","expected_identity_id":f.identity,"expected_device_id":f.device,"item_ids":[item,item]});
    let response = f.engine.client_request(request.to_string()).await.unwrap();
    let rows: Vec<NativeSurfaceItemDto> = serde_json::from_str(&response).unwrap();
    assert_eq!(rows.len(), 1);
    let before = f.counts();
    let mut stale = request;
    stale["expected_device_id"] = json!("old_device");
    assert!(f.engine.client_request(stale.to_string()).await.is_err());
    assert_eq!(f.counts(), before);
}
