//! Real signed canonical grants, decisions and cold replay; no cached permission map.
use super::*;
use types::{ActionClass, Capability, EventBody, Grant, GrantScope, TrustLevel};

fn context(c: &Client) -> (String, String) {
    (agent(c, "Zoen").id, find_space(c, "Paraty com a Marina").id)
}
fn issue(
    c: &Client,
    agent: &str,
    space: &str,
    capability: Capability,
    expiry: Option<i64>,
) -> String {
    let mut e = c.lock();
    let owner = e.me_id().unwrap();
    let id = types::new_id("grant_test");
    e.append(
        space,
        &owner,
        EventBody::GrantIssued {
            grant: Grant {
                id: id.clone(),
                grantor: owner.clone(),
                grantee: Some(agent.into()),
                scope: GrantScope::Space(space.into()),
                capability,
                expires_at_ms: expiry,
            },
        },
    )
    .unwrap();
    id
}
fn revoke(c: &Client, space: &str, grant: &str) {
    let mut e = c.lock();
    let owner = e.me_id().unwrap();
    e.append(
        space,
        &owner,
        EventBody::GrantRevoked {
            grant: grant.into(),
        },
    )
    .unwrap();
}
fn decision(c: &Client, agent: &str, space: &str) -> grants::Decision {
    c.lock().decide(agent, space, &ActionClass::Reversible, 0)
}
#[test]
fn newest_revoked_trust_blocks_actions_and_reply_without_reviving_old_autonomy() {
    let c = seeded();
    let (agent, space) = context(&c);
    issue(
        &c,
        &agent,
        &space,
        Capability::Trust(TrustLevel::Autonomous),
        None,
    );
    let lower = issue(
        &c,
        &agent,
        &space,
        Capability::Trust(TrustLevel::Suggest),
        None,
    );
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Request(_)
    ));
    revoke(&c, &space, &lower);
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Block(_)
    ));
    assert!(matches!(
        c.lock().decide(&agent, &space, &ActionClass::Reply, 0),
        grants::Decision::Block(_)
    ));
    assert_eq!(c.lock().trust(&agent, &space), TrustLevel::Listen);
    assert!(c.verify_all().iter().all(|r| r.valid));
}
#[test]
fn explicit_regrant_restores_authority_but_expired_newest_regrant_cannot() {
    let c = seeded();
    let (agent, space) = context(&c);
    let first = issue(&c, &agent, &space, Capability::Trust(TrustLevel::Act), None);
    revoke(&c, &space, &first);
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Block(_)
    ));
    issue(
        &c,
        &agent,
        &space,
        Capability::Trust(TrustLevel::Suggest),
        None,
    );
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Request(_)
    ));
    issue(
        &c,
        &agent,
        &space,
        Capability::Trust(TrustLevel::Act),
        Some(crate::engine::now_ms() - 1),
    );
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Block(_)
    ));
    issue(&c, &agent, &space, Capability::Trust(TrustLevel::Act), None);
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Act { .. }
    ));
}
#[test]
fn pending_approval_does_not_regrant_revoked_trust_and_can_still_be_denied() {
    let c = seeded();
    let (agent, space) = context(&c);
    let baseline = issue(
        &c,
        &agent,
        &space,
        Capability::Trust(TrustLevel::Suggest),
        None,
    );
    let request = c
        .lock()
        .open_request(
            &space,
            &agent,
            "Reviewed action",
            "Same content",
            "Space",
            ActionClass::Irreversible,
            None,
        )
        .unwrap()
        .unwrap();
    revoke(&c, &space, &baseline);
    let before = c.stats().events;
    assert!(c.approve_request(request.clone()).is_err());
    assert_eq!(c.stats().events, before);
    c.deny_request(request).unwrap();
}
#[test]
fn invalid_newest_standing_baseline_does_not_revive_previous_always_allow() {
    let c = seeded();
    let (agent, space) = context(&c);
    issue(
        &c,
        &agent,
        &space,
        Capability::Trust(TrustLevel::Suggest),
        None,
    );
    let action = grants::standing_key(&ActionClass::Reversible).to_string();
    issue(
        &c,
        &agent,
        &space,
        Capability::Standing {
            action: action.clone(),
            allow: true,
        },
        None,
    );
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Act { .. }
    ));
    let deny = issue(
        &c,
        &agent,
        &space,
        Capability::Standing {
            action: action.clone(),
            allow: false,
        },
        None,
    );
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Block(_)
    ));
    revoke(&c, &space, &deny);
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Request(_)
    ));
    issue(
        &c,
        &agent,
        &space,
        Capability::Standing {
            action,
            allow: true,
        },
        Some(crate::engine::now_ms() - 1),
    );
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Request(_)
    ));
}
#[test]
fn revoked_baseline_survives_cold_replay_of_the_same_signed_logs() {
    let dir = std::env::temp_dir().join(types::new_id("trust_reopen"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().into_owned();
    let (agent, space) = {
        let c = Client::open(path.clone(), "pt-BR".into()).unwrap();
        c.seed_demo_if_empty().unwrap();
        let (agent, space) = context(&c);
        let grant = issue(
            &c,
            &agent,
            &space,
            Capability::Trust(TrustLevel::Suggest),
            None,
        );
        revoke(&c, &space, &grant);
        (agent, space)
    };
    let c = Client::open(path, "pt-BR".into()).unwrap();
    assert!(matches!(
        decision(&c, &agent, &space),
        grants::Decision::Block(_)
    ));
    assert!(c.verify_all().iter().all(|r| r.valid));
    drop(c);
    std::fs::remove_dir_all(dir).unwrap();
}

fn app_grant(
    c: &Client,
    space: &str,
    item: &str,
    level: TrustLevel,
    expiry: Option<i64>,
) -> String {
    let mut e = c.lock();
    let owner = e.me_id().unwrap();
    let id = types::new_id("app_trust_test");
    e.append(
        space,
        &owner,
        EventBody::GrantIssued {
            grant: Grant {
                id: id.clone(),
                grantor: owner.clone(),
                grantee: Some(format!("app:{item}")),
                scope: GrantScope::Item(item.into()),
                capability: Capability::Trust(level),
                expires_at_ms: expiry,
            },
        },
    )
    .unwrap();
    id
}
#[test]
fn invalid_newest_app_baseline_blocks_mutation_until_explicit_regrant() {
    for expired in [false, true] {
        let c = seeded();
        let (_, space) = context(&c);
        let item = c
            .install_app(
                space.clone(),
                "pet".into(),
                r#"{"name":"Trust test"}"#.into(),
            )
            .unwrap();
        assert_eq!(c.lock().app_trust(&item.id), TrustLevel::Act);
        let lower = app_grant(
            &c,
            &space,
            &item.id,
            TrustLevel::Suggest,
            expired.then(|| crate::engine::now_ms() - 1),
        );
        if !expired {
            revoke(&c, &space, &lower);
        }
        let before = c.item(item.id.clone()).unwrap().version;
        assert_eq!(c.lock().app_trust(&item.id), TrustLevel::Listen);
        let denied = c
            .app_call_tool(item.id.clone(), "pet_feed".into(), "{}".into(), true)
            .unwrap();
        assert_eq!(
            denied.status,
            AppCallStatus::Denied,
            "confirmation cannot regrant an invalid baseline"
        );
        assert_eq!(c.item(item.id.clone()).unwrap().version, before);
        app_grant(&c, &space, &item.id, TrustLevel::Act, None);
        assert_eq!(
            c.app_call_tool(item.id.clone(), "pet_feed".into(), "{}".into(), false)
                .unwrap()
                .status,
            AppCallStatus::Done
        );
        assert!(c.item(item.id).unwrap().version > before);
    }
}

#[test]
fn app_confirmation_cannot_override_invalid_trust_for_an_external_action() {
    let c = seeded();
    let (_, space) = context(&c);
    let item = c
        .install_app(
            space.clone(),
            "pet".into(),
            r#"{"name":"Confirm test"}"#.into(),
        )
        .unwrap();
    let lower = app_grant(&c, &space, &item.id, TrustLevel::Suggest, None);
    revoke(&c, &space, &lower);
    let before = c.item(item.id.clone()).unwrap().version;
    let refused = c
        .app_call_tool(item.id.clone(), "pet_release".into(), "{}".into(), true)
        .unwrap();
    assert_eq!(refused.status, AppCallStatus::Denied);
    assert_eq!(c.item(item.id).unwrap().version, before);
}
#[test]
fn app_grant_cannot_replace_the_calling_agents_revoked_authority() {
    let c = seeded();
    let (agent, space) = context(&c);
    let item = c
        .install_app(
            space.clone(),
            "pet".into(),
            r#"{"name":"Agent test"}"#.into(),
        )
        .unwrap();
    let agent_grant = issue(&c, &agent, &space, Capability::Trust(TrustLevel::Act), None);
    let owner = c.me().unwrap().id;
    let first = c
        .lock()
        .app_call_as(&item.id, &agent, "pet_feed", "{}", false, Some(&owner))
        .unwrap();
    assert_eq!(first.status, AppCallStatus::Done);
    revoke(&c, &space, &agent_grant);
    let before = c.item(item.id.clone()).unwrap().version;
    let refused = c
        .lock()
        .app_call_as(&item.id, &agent, "pet_feed", "{}", false, Some(&owner))
        .unwrap();
    assert_eq!(refused.status, AppCallStatus::Denied);
    assert_eq!(c.item(item.id).unwrap().version, before);
}
