//! Same checked history with optimistic, waiting-plan and membership fences.
use super::*;
use crate::engine::Engine;
use crate::sync::{
    replay_probe::{page_body, projection, Home},
    Ingest,
};
use event_log::{Author, Signer};
use protocol::Envelope;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const SPACE: &str = "sp_remote_page_catchup";

struct Fixture {
    home: Home,
    path: std::path::PathBuf,
    engine: Engine,
    owner: Author,
}

impl Fixture {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let home = Home(std::env::temp_dir().join(types::new_id("page-overlay")?));
        std::fs::create_dir(&home.0).unwrap();
        let path = home.0.join("reader.sqlite");
        let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
        engine
            .create_account("Reader", "reader", "http://127.0.0.1:9")
            .unwrap();
        let owner = Author::device(&Signer::generate()?, Signer::generate()?);
        engine
            .put_profiles(vec![Identity {
                id: owner.identity.clone(),
                kind: IdentityKind::Person,
                name: "Remote owner".into(),
                handle: "remote-owner".into(),
                tint_hex: "#778899".into(),
                glyph: None,
                owner: None,
                owner_proof: None,
                bio: String::new(),
            }])
            .unwrap();
        let mut f = Self {
            home,
            path,
            engine,
            owner,
        };
        f.append(EventBody::SpaceCreated {
            title: "Remote pages".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        })?;
        f.append(EventBody::MemberAdded {
            identity: f.engine.me_id().unwrap(),
            role: Role::Member,
        })?;
        Ok(f)
    }

    fn append_as(
        &mut self,
        author: &Author,
        body: EventBody,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut head = self.engine.logs.get(SPACE).and_then(|log| log.head());
        let event = author.sign_event(
            SPACE,
            &types::new_id("page-event")?,
            1_800_000_000_000,
            head.clone(),
            body,
        )?;
        assert_eq!(
            self.engine
                .ingest(frame(Envelope::plain(&event), &mut head)?),
            Ingest::Applied
        );

        Ok(())
    }

    fn append(&mut self, body: EventBody) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.append_as(&self.owner.clone(), body)?;

        Ok(())
    }

    fn page(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let body = page_body(0);
        self.append(body)?;

        Ok(())
    }

    fn legacy_plan(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.append(EventBody::ItemCreated {
            item: "legacy-plan".into(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(PlanDoc {
                title: "Retained plan".into(),
                summary: "Exact prefix".into(),
                budget_cents: Some(100),
                sections: vec![PlanSection {
                    title: "Work".into(),
                    lines: vec![PlanLine {
                        id: "line".into(),
                        text: "Owner review".into(),
                        cost_cents: 37,
                        done: false,
                    }],
                }],
            }),
            origin: "User".into(),
        })?;

        Ok(())
    }

    fn assert_cold(self) {
        let before = snapshot(&self.engine);
        let Self {
            home, path, engine, ..
        } = self;
        drop(engine);
        let cold = Engine::open(path.to_str().unwrap()).unwrap();
        assert_eq!(snapshot(&cold), before);
        assert!(cold.logs[SPACE].verify().is_ok());
        drop(cold);
        drop(home);
    }
}

fn snapshot(engine: &Engine) -> serde_json::Value {
    let debug_map = |values: Vec<(String, String)>| values.into_iter().collect::<BTreeMap<_, _>>();
    serde_json::json!({
        "items": projection(engine),
        "spaces": debug_map(engine.state.spaces.iter().map(|(key, value)| (key.clone(), format!("{value:?}"))).collect()),
        "requests": debug_map(engine.state.requests.iter().map(|(key, value)| (key.clone(), format!("{value:?}"))).collect()),
        "planOrigins": debug_map(engine.state.plan_origins.iter().map(|(key, value)| (key.clone(), format!("{value:?}"))).collect()),
        "waiting": engine.state.waiting_spaces.iter().cloned().collect::<std::collections::BTreeSet<_>>(),
        "requestOrder": engine.state.request_order, "resolvedOrder": engine.state.resolved_order,
        "grants": format!("{:?}", engine.state.grants), "usage": engine.state.usage,
        "outbox": engine.store.outbox().unwrap().iter().map(|row| (&row.event, row.failed)).collect::<Vec<_>>(),
        "cursor": engine.next_seq(SPACE)
    })
}

#[test]
fn ordinary_remote_pages_retain_pending_and_failed_outbox_projection_after_cold_replay() {
    for failed in [false, true] {
        let mut f = Fixture::new().unwrap();
        let pending = f
            .engine
            .send_message(SPACE, "Retained local intent")
            .unwrap();
        if failed {
            let result = f
                .engine
                .reject(&pending.id, "synthetic terminal refusal", true, None);
            assert!(result.user_failed());
        }
        assert_eq!(f.engine.store.outbox().unwrap().len(), 1);
        crate::sync::replay_probe::reset();
        f.page().unwrap();
        assert!(
            crate::sync::replay_probe::steps() > 0,
            "all outbox rows, including failed recovery, retain the original full replay"
        );
        assert_eq!(f.engine.state.items.len(), 1);
        assert_eq!(f.engine.store.outbox().unwrap()[0].failed, failed);
        f.assert_cold();
    }
}

#[test]
fn ordinary_remote_page_after_author_removal_still_uses_checked_projection() {
    let mut f = Fixture::new().unwrap();
    let removed = Author::device(&Signer::generate().unwrap(), Signer::generate().unwrap());
    f.append(EventBody::MemberAdded {
        identity: removed.identity.clone(),
        role: Role::Member,
    })
    .unwrap();
    f.append(EventBody::MemberRemoved {
        identity: removed.identity.clone(),
    })
    .unwrap();
    f.owner = removed;
    crate::sync::replay_probe::reset();
    f.page().unwrap();
    assert_eq!(crate::sync::replay_probe::steps(), 0);
    assert!(
        f.engine.state.items.is_empty(),
        "a verified chain is not writer authority after removal"
    );
    f.assert_cold();
}

#[test]
fn ordinary_remote_page_beside_a_confirmed_legacy_plan_matches_cold_projection() {
    let mut f = Fixture::new().unwrap();
    f.legacy_plan().unwrap();
    crate::sync::replay_probe::reset();
    f.page().unwrap();
    assert_eq!(crate::sync::replay_probe::steps(), 0);
    assert_eq!(f.engine.state.items.len(), 2);
    f.assert_cold();
}

#[test]
fn ordinary_remote_page_preserves_a_real_optimistic_plan_approval_until_its_echo(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new().unwrap();
    f.legacy_plan().unwrap();
    let local = f.engine.net.author.clone().unwrap();
    let agent = Author::device(&Signer::generate().unwrap(), Signer::generate().unwrap());
    f.engine
        .put_profiles(vec![Identity {
            id: agent.identity.clone(),
            kind: IdentityKind::Agent,
            name: "Local owner's reviewer".into(),
            handle: "local-reviewer".into(),
            tint_hex: "#778899".into(),
            glyph: None,
            owner: Some(local.identity.clone()),
            owner_proof: Some(Box::new(
                event_log::agent_owner::authorize(&local, &agent.identity).unwrap(),
            )),
            bio: String::new(),
        }])
        .unwrap();
    f.append(EventBody::MemberAdded {
        identity: agent.identity.clone(),
        role: Role::Member,
    })
    .unwrap();
    let fold = f.engine.plan_timeline("legacy-plan").unwrap();
    let plan = fold
        .document
        .as_ref()
        .unwrap()
        .view()
        .unwrap()
        .to_plan_doc();
    let line = &plan.sections[0].lines[0];
    let request = AgentRequest {
        id: "optimistic-request".into(),
        agent: agent.identity.clone(),
        title: "Owner decision".into(),
        detail: "Exact retained row".into(),
        audience: "This Space".into(),
        action: ActionClass::Money {
            cents: line.cost_cents,
        },
        content_hash: event_log::content_hash(&(&line.text, line.cost_cents))?,
        item: Some("legacy-plan".into()),
        line: Some(line.id.clone()),
        proposal: None,
        tool_proposal: None,
    };
    f.append_as(
        &agent,
        EventBody::RequestOpened {
            request: request.clone(),
        },
    )
    .unwrap();
    f.engine
        .decide_request(&request.id, crate::dto::RequestDecision::Approve)
        .unwrap();
    assert!(f.engine.net.optimistic_approvals.contains(SPACE));
    assert_eq!(f.engine.store.outbox_len().unwrap(), 1);
    let pending = f.engine.store.outbox().unwrap()[0].event.clone();
    crate::sync::replay_probe::reset();
    f.page().unwrap();
    assert!(crate::sync::replay_probe::steps() > 0);
    assert_eq!(
        f.engine.state.requests[&request.id].status,
        crate::engine::ReqStatus::Approved
    );
    assert_eq!(f.engine.store.outbox().unwrap()[0].event, pending);
    assert!(f.engine.state.grants.is_empty());
    f.assert_cold();
    Ok(())
}

#[test]
fn ordinary_remote_page_does_not_release_a_real_waiting_plan_approval(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new().unwrap();
    f.legacy_plan().unwrap();
    let agent = Author::device(&Signer::generate().unwrap(), Signer::generate().unwrap());
    f.engine
        .put_profiles(vec![Identity {
            id: agent.identity.clone(),
            kind: IdentityKind::Agent,
            name: "Plan reviewer".into(),
            handle: "reviewer".into(),
            tint_hex: "#778899".into(),
            glyph: None,
            owner: Some(f.owner.identity.clone()),
            owner_proof: Some(Box::new(
                event_log::agent_owner::authorize(&f.owner, &agent.identity).unwrap(),
            )),
            bio: String::new(),
        }])
        .unwrap();
    f.append(EventBody::MemberAdded {
        identity: agent.identity.clone(),
        role: Role::Member,
    })
    .unwrap();
    let fold = f.engine.plan_timeline("legacy-plan").unwrap();
    let document = fold.document.as_ref().unwrap();
    let plan = document.view().unwrap().to_plan_doc();
    let bytes = document.full_update().to_json().unwrap();
    let (key, sealed) = crate::media::seal(&bytes).unwrap();
    let content = PageContent {
        title: plan.title.clone(),
        path: String::new(),
        loro: String::new(),
        blob: Some(ChunkRef {
            sha256: hex::encode(Sha256::digest(&bytes)),
            bytes: bytes.len() as u64,
            blob: hex::encode(Sha256::digest(&sealed)),
            key: hex::encode(key),
        }),
        manifest: Some(PageManifest::PlanV1(fold.manifest().unwrap())),
    };
    f.append(EventBody::ItemVersioned {
        item: "legacy-plan".into(),
        content: ItemContent::Page(content),
        note: "Missing authenticated payload".into(),
    })
    .unwrap();
    let line = &plan.sections[0].lines[0];
    let request = AgentRequest {
        id: "waiting-request".into(),
        agent: agent.identity.clone(),
        title: "Owner decision".into(),
        detail: "Exact retained row".into(),
        audience: "This Space".into(),
        action: ActionClass::Money {
            cents: line.cost_cents,
        },
        content_hash: event_log::content_hash(&(&line.text, line.cost_cents))?,
        item: Some("legacy-plan".into()),
        line: Some(line.id.clone()),
        proposal: None,
        tool_proposal: None,
    };
    f.append_as(
        &agent,
        EventBody::RequestOpened {
            request: request.clone(),
        },
    )
    .unwrap();
    f.append(EventBody::RequestResolved {
        request: request.id.clone(),
        approved: true,
        content_hash: request.content_hash,
        resolution: Some(RequestResolution {
            message: "Owner decision recorded".into(),
            created_item: None,
            ai_cost_cents: 0,
            standing_grant: None,
            tool_decision: None,
        }),
    })
    .unwrap();
    assert!(
        f.engine.state.waiting_spaces.contains(SPACE),
        "the authenticated missing payload actually holds the decision"
    );
    assert_eq!(
        f.engine.state.requests[&request.id].status,
        crate::engine::ReqStatus::Pending
    );
    crate::sync::replay_probe::reset();
    f.page().unwrap();
    assert!(crate::sync::replay_probe::steps() > 0);
    assert!(f.engine.state.waiting_spaces.contains(SPACE));
    assert!(!f.engine.state.items.contains_key("remote-item-0"));
    assert!(f.engine.state.grants.is_empty());
    assert!(f.engine.state.usage.is_empty());
    f.assert_cold();
    Ok(())
}

fn frame(
    env: Envelope,
    head: &mut Option<Seen>,
) -> std::result::Result<protocol::Sequenced, Box<dyn std::error::Error>> {
    let seq = head.as_ref().map_or(0, |h| h.seq + 1);
    let prev = head
        .as_ref()
        .map_or_else(|| GENESIS_PREV.to_string(), |h| h.hash.clone());
    let hash = event_log::chain_hash(env.space(), seq, &prev, &env.wire_hash())?;
    *head = Some(Seen {
        seq,
        hash: hash.clone(),
    });
    Ok(protocol::Sequenced {
        seq,
        prev,
        hash,
        env,
    })
}
