use super::*;
use serde_json::{json, Value};

#[test]
fn living_space_default_off_and_signed_metadata_survives_replay() {
    let e = seeded();
    let space = e.create_community("Living test".into()).unwrap();
    let query = json!({"action":"home"}).to_string();
    assert!(e
        .living_space_command(space.clone(), query.clone())
        .is_err());
    e.set_living_space_enabled(true).unwrap();
    let page = e.page_create(space.clone(), "Home".into()).unwrap();
    e.living_space_command(
        space.clone(),
        json!({"action":"set_home","page_id":page.id}).to_string(),
    )
    .unwrap();
    e.living_space_command(
        space.clone(),
        json!({"action":"announce","item_id":page.id}).to_string(),
    )
    .unwrap();
    let home: Value = serde_json::from_str(
        &e.living_space_command(space.clone(), query.clone())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(home["page"], page.id);
    assert_eq!(home["announcement_items"], json!([page.id]));
    assert_eq!(home["pending_requests"], json!([]));
    let mut engine = e.lock();
    assert!(engine
        .space_state(&space)
        .unwrap()
        .entries
        .iter()
        .all(|entry| !format!("{:?}", entry.body).contains("zoen-living-space-meta")));
    let events = engine.store.events(&space).unwrap();
    assert!(events.iter().any(|e| matches!(&e.body, roda_types::EventBody::MessagePosted { text, .. } if text.starts_with(crate::living_space::META_PREFIX))));
    // Replay reads the durable signed log, not an in-memory UI model or cached roles.
    engine
        .store
        .set_meta("experiment.living_space.enabled", "false")
        .unwrap();
    assert!(engine.living_space_command(&space, &query).is_err());
    engine
        .store
        .set_meta("experiment.living_space.enabled", "true")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&engine.living_space_command(&space, &query).unwrap())
            .unwrap()["page"],
        page.id
    );
}

#[test]
fn living_space_hackathon_uses_actual_mls_team_and_one_winners_announcement() {
    use crate::engine::{now_ms, Engine};
    use roda_types::{EventBody, Privacy, SpaceKind};
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Alice", "alice_hack", "http://relay.test")
        .unwrap();
    engine
        .store
        .set_meta("experiment.living_space.enabled", "true")
        .unwrap();
    let root = engine
        .create_synced_space("Build", SpaceKind::Community, Privacy::Closed, &[])
        .unwrap();
    let deadline = now_ms() + 60_000;
    engine
        .living_space_command(
            &root,
            &json!({"action":"hackathon_create","title":"Build","deadline_ms":deadline})
                .to_string(),
        )
        .unwrap();
    engine
        .living_space_command(&root, &json!({"action":"register"}).to_string())
        .unwrap();
    engine
        .living_space_command(
            &root,
            &json!({"action":"team_create","name":"Builders"}).to_string(),
        )
        .unwrap();
    let hack = engine.living_hackathon_replay(&root).unwrap();
    let team = &hack.teams[0];
    assert_eq!(
        engine.space_state(&team.space).unwrap().privacy,
        Privacy::EndToEnd
    );
    assert!(
        engine.mls_status(&team.space).is_some(),
        "actual MLS group exists, not just a metadata label"
    );
    engine.living_space_command(&root,&json!({"action":"project_submit","team_id":team.id,"url":"https://example.com/demo","file_ids":[]}).to_string()).unwrap();
    assert!(
        engine
            .living_space_command(&root, &json!({"action":"finalize"}).to_string())
            .is_err(),
        "cannot finalize before authoritative phase transition"
    );
    let me = engine.me_id().unwrap();
    engine
        .append_at(
            &root,
            &me,
            deadline + 1,
            EventBody::MessagePosted {
                message: roda_types::new_id("msg"),
                text: format!(
                    "{}{}",
                    crate::living_hackathon::HACK_META,
                    json!({"action":"finalize"})
                ),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        )
        .unwrap();
    assert!(
        !engine.living_hackathon_replay(&root).unwrap().finalized,
        "a future authored timestamp cannot finalize the open phase"
    );
    engine
        .living_space_command(&root, &json!({"action":"submissions_close"}).to_string())
        .unwrap();
    for _ in 0..2 {
        engine
            .living_space_command(&root, &json!({"action":"finalize"}).to_string())
            .unwrap();
    }
    let result: Value = serde_json::from_str(
        &engine
            .living_space_command(&root, &json!({"action":"home"}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(result["announcement_items"].as_array().unwrap().len(), 1);
    assert_eq!(
        engine
            .state
            .items
            .values()
            .filter(|i| i.origin == "Hackathon results")
            .count(),
        1
    );
}

#[test]
fn living_space_signed_demo_request_approval_changes_actual_membership() {
    let e = seeded();
    e.set_living_space_enabled(true).unwrap();
    let root = e.create_community("Join fixture".into()).unwrap();
    e.living_space_command(
        root.clone(),
        json!({"action":"demo_join_request"}).to_string(),
    )
    .unwrap();
    let before: Value = serde_json::from_str(
        &e.living_space_command(root.clone(), json!({"action":"home"}).to_string())
            .unwrap(),
    )
    .unwrap();
    let applicant = before["pending_requests"][0].as_str().unwrap().to_owned();
    assert!(!e
        .space(root.clone())
        .unwrap()
        .members
        .iter()
        .any(|m| m.id == applicant));
    e.living_space_command(
        root.clone(),
        json!({"action":"approve_join","identity_id":applicant}).to_string(),
    )
    .unwrap();
    assert!(e
        .space(root.clone())
        .unwrap()
        .members
        .iter()
        .any(|m| m.id == applicant));
    let after: Value = serde_json::from_str(
        &e.living_space_command(root, json!({"action":"home"}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(after["pending_requests"], json!([]));
}

#[test]
fn living_space_demo_request_rejects_real_account() {
    let e = seeded();
    e.lock()
        .create_account("Owner", "real_owner", "http://relay.test")
        .unwrap();
    e.set_living_space_enabled(true).unwrap();
    let root = e.create_community("Real account".into()).unwrap();
    assert!(e
        .living_space_command(root, json!({"action":"demo_join_request"}).to_string())
        .is_err());
}

#[test]
fn living_space_small_cap_splits_admission_into_real_mls_partitions() {
    use crate::engine::Engine;
    use roda_types::{Privacy, Role, SpaceKind};
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Owner", "owner_partition", "http://relay.test")
        .unwrap();
    engine
        .store
        .set_meta("experiment.living_space.enabled", "true")
        .unwrap();
    let root = engine
        .create_synced_space("Bounded", SpaceKind::Community, Privacy::Closed, &[])
        .unwrap();
    engine
        .living_space_command(
            &root,
            &json!({"action":"conversation_policy","member_cap":2}).to_string(),
        )
        .unwrap();
    let a = roda_log::Signer::from_secret(&[81; 32]).id();
    let b = roda_log::Signer::from_secret(&[82; 32]).id();
    engine.add_member(&root, &a, Role::Member).unwrap();
    engine.add_member(&root, &b, Role::Member).unwrap();
    let (cap, groups) = engine.living_conversation_policy(&root);
    assert_eq!(cap, 2);
    assert_eq!(groups.len(), 2);
    for child in &groups {
        assert_eq!(
            engine.space_state(child).unwrap().privacy,
            Privacy::EndToEnd
        );
        assert!(engine.space_state(child).unwrap().members.len() <= 2);
        assert!(engine.mls_status(child).is_some());
        assert!(!engine.spaces().iter().any(|s| &s.id == child));
    }
    assert_ne!(
        engine.living_conversation_space(&root, &a),
        engine.living_conversation_space(&root, &b)
    );
    assert!(engine
        .send_message(&root, "must not be plaintext in parent")
        .is_err());
    assert!(engine
        .living_space_command(
            &root,
            &json!({"action":"conversation_policy","member_cap":1}).to_string()
        )
        .is_err());
    engine.remove_member(&root, &a).unwrap();
    assert!(engine.living_conversation_space(&root, &a).is_none());
    assert!(groups.iter().all(|g| !engine.is_member(g, &a)));
    engine
        .store
        .set_meta("experiment.living_space.enabled", "false")
        .unwrap();
    assert!(
        engine.spaces().iter().any(|s| s.id == groups[0]),
        "disabling experiment restores legacy child visibility"
    );
    assert!(engine
        .send_message(&root, "legacy parent messaging restored")
        .is_ok());
    engine
        .store
        .set_meta("experiment.living_space.enabled", "true")
        .unwrap();
    assert!(!engine.spaces().iter().any(|s| s.id == groups[0]));
    assert!(engine
        .send_message(&root, "living parent plaintext refused")
        .is_err());
    // A root removal can arrive before the corresponding child commit. Retained
    // cached child membership must not authorize opening or sending into the child.
    let me = engine.me_id().unwrap();
    engine
        .state
        .spaces
        .get_mut(&root)
        .unwrap()
        .members
        .retain(|(id, _)| id != &me);
    assert!(engine.is_member(&groups[0], &me));
    assert!(engine.space_summary(&groups[0]).is_err());
    assert!(engine.timeline(&groups[0]).is_err());
    assert!(engine
        .send_message(&groups[0], "stale membership must not send")
        .is_err());
}

#[test]
fn living_space_anonymous_ordinary_reads_survive_but_protected_children_refuse() {
    use roda_types::{EventBody, Privacy, SpaceKind};
    let e = seeded();
    let root = e.create_community("Protected root".into()).unwrap();
    let mut engine = e.lock();
    let me = engine.me_id().unwrap();
    let child = roda_types::new_id("sp");
    engine
        .append(
            &child,
            &me,
            EventBody::SpaceCreated {
                title: "Hidden".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        )
        .unwrap();
    engine
        .append(
            &root,
            &me,
            EventBody::MessagePosted {
                message: roda_types::new_id("msg"),
                text: format!(
                    "{}{}",
                    crate::living_space::META_PREFIX,
                    json!({"action":"register_child","child":child})
                ),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        )
        .unwrap();
    engine.me = None;
    assert!(engine.space_summary(&root).is_ok());
    assert!(engine.timeline(&root).is_ok());
    assert!(engine.space_summary(&child).is_err());
    assert!(engine.timeline(&child).is_err());
}

#[test]
fn living_space_rejects_cross_space_page_and_nonadmin_metadata() {
    let e = seeded();
    e.set_living_space_enabled(true).unwrap();
    let space = e.create_community("Living test".into()).unwrap();
    let other = e.create_community("Other".into()).unwrap();
    let page = e.page_create(other, "Wrong parent".into()).unwrap();
    assert!(e
        .living_space_command(
            space.clone(),
            json!({"action":"set_home","page_id":page.id}).to_string()
        )
        .is_err());
    let mut engine = e.lock();
    let me = engine.me_id().unwrap();
    // Exercise the adapter's authority from projected roles, not a client-supplied role.
    engine
        .state
        .spaces
        .get_mut(&space)
        .unwrap()
        .members
        .iter_mut()
        .find(|(id, _)| id == &me)
        .unwrap()
        .1 = roda_types::Role::Member;
    assert!(engine
        .living_space_command(
            &space,
            &json!({"action":"announce","item_id":page.id}).to_string()
        )
        .is_err());
    assert!(engine
        .living_space_command(
            &space,
            &json!({"action":"approve_join","identity_id":"stranger"}).to_string()
        )
        .is_err());
    assert!(engine
        .living_space_command(
            &space,
            &json!({"action":"create_child","title":"secret"}).to_string()
        )
        .is_err());
    assert!(engine
        .living_space_command(&space, &json!({"action":"demo_join_request"}).to_string())
        .is_err());
}
