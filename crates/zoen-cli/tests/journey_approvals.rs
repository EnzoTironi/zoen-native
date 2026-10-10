//! Real relay/FDB ordering, device admission, SQLite interruption and cold restart.
//! These are core approval effects, not cloud model or provider billing proof.

mod common;
use common::{RawClient, World};
use roda_ffi::{Engine, Ingest, RequestDecision, RequestStatus};
use roda_log::{Author, Signer};
use roda_proto::{DeviceCertificate, Op, Reply, Sequenced};
use roda_types::*;
use std::path::Path;
use std::time::Duration;

struct Owner {
    root: Signer,
    author: Author,
    device_secret: [u8; 32],
}

fn reopen(path: &Path, owner: &Owner) -> Engine {
    let mut e = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(e.unlock(Some(owner.device_secret.to_vec())).unwrap());
    e
}

fn ingest(e: &mut Engine, event: Sequenced) {
    assert!(matches!(
        e.ingest(event),
        Ingest::Applied | Ingest::Confirmed | Ingest::Duplicate
    ));
}

async fn setup(w: &World, path: &Path) -> (Engine, Owner, RawClient, RawClient, String) {
    let mut e = Engine::open(path.to_str().unwrap()).unwrap();
    let (root, secret, _) = e.create_account("Owner", "owner", &w.relay_url()).unwrap();
    let owner = Owner {
        root: Signer::from_secret(&root),
        author: Author::device(&Signer::from_secret(&root), Signer::from_secret(&secret)),
        device_secret: secret,
    };
    let mut person = RawClient::reconnect(&w.relay_url(), owner.author.clone()).await;
    assert!(matches!(
        person
            .request(Op::Register {
                profile: e.my_profile().unwrap()
            })
            .await,
        Ok(Reply::Registered(_))
    ));
    e.set_registered(true).unwrap();
    let author = Author::device(&Signer::generate(), Signer::generate());
    let mut agent = RawClient::reconnect(&w.relay_url(), author).await;
    let profile = Identity {
        id: agent.identity(),
        kind: IdentityKind::Agent,
        name: "Approval agent".into(),
        handle: "approval_agent".into(),
        tint_hex: "#4F7CFF".into(),
        glyph: None,
        owner: Some(person.identity()),
        bio: String::new(),
    };
    let Ok(Reply::Registered(profile)) = agent.request(Op::Register { profile }).await else {
        panic!("agent registration")
    };
    e.put_profiles(vec![profile]).unwrap();
    let space = new_id("sp");
    for body in [
        EventBody::SpaceCreated {
            title: "Durable approvals".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        EventBody::MemberAdded {
            identity: agent.identity(),
            role: Role::Member,
        },
        EventBody::GrantIssued {
            grant: Grant {
                id: new_id("gr"),
                grantor: person.identity(),
                grantee: Some(agent.identity()),
                scope: GrantScope::Space(space.clone()),
                capability: Capability::Trust(TrustLevel::Suggest),
                expires_at_ms: None,
            },
        },
        EventBody::GrantIssued {
            grant: Grant {
                id: new_id("gr"),
                grantor: person.identity(),
                grantee: Some(agent.identity()),
                scope: GrantScope::Everywhere,
                capability: Capability::MonthlyBudget { cents: 1000 },
                expires_at_ms: None,
            },
        },
    ] {
        let event = person
            .publish_body(&space, e.causal_head(&space).unwrap(), body)
            .await
            .unwrap();
        ingest(&mut e, event);
    }
    (e, owner, person, agent, space)
}

async fn propose(e: &mut Engine, agent: &mut RawClient, space: &str, title: &str) -> AgentRequest {
    let mut request = AgentRequest {
        id: new_id("rq"),
        agent: agent.identity(),
        title: title.into(),
        detail: "Exact retained content".into(),
        audience: "Members of this Space".into(),
        action: ActionClass::Reversible,
        content_hash: String::new(),
        item: None,
        line: None,
        proposal: Some(Box::new(ItemProposal {
            item: new_id("it"),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(PlanDoc {
                title: title.into(),
                summary: "Owner review".into(),
                budget_cents: None,
                sections: vec![PlanSection {
                    title: "Work".into(),
                    lines: vec![PlanLine {
                        id: new_id("line"),
                        text: "Keep these exact bytes".into(),
                        cost_cents: 4200,
                        done: false,
                    }],
                }],
            }),
            origin: "Scripted proposal fixture".into(),
            message: format!("Created {title}"),
            ai_cost_cents: 37,
        })),
    };
    request.content_hash = request.proposal_hash(space).unwrap();
    let seen = e.causal_head(space).unwrap();
    ingest(
        e,
        agent
            .publish_body(
                space,
                seen,
                EventBody::RequestOpened {
                    request: request.clone(),
                },
            )
            .await
            .unwrap(),
    );
    request
}

fn fault(path: &Path, sql: &str) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch(sql)
        .unwrap();
}

async fn publish_after_restart(person: &mut RawClient, env: roda_proto::Envelope) -> Sequenced {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match person.publish_env(env.clone()).await {
            Ok(event) => return event,
            Err(reason) => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "restarted relay did not acquire its fenced partition: {reason}"
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

#[tokio::test]
async fn interrupted_approval_batch_and_lost_ack_survive_core_and_relay_restarts() {
    let mut w = World::with_nats("approval_restart").await;
    let path = w.dir.join("owner.sqlite");
    let (mut e, owner, person, mut agent, space) = setup(&w, &path).await;
    let first = propose(&mut e, &mut agent, &space, "First durable plan").await;
    let second = propose(&mut e, &mut agent, &space, "Second durable plan").await;
    drop(person);
    drop(agent);
    w.stop_relay();
    drop(e);
    fault(&path, &format!("CREATE TRIGGER fail_covered BEFORE INSERT ON outbox WHEN NEW.json LIKE '%RequestResolved%' AND NEW.json LIKE '%{}%' BEGIN SELECT RAISE(ABORT, 'covered approval fault'); END;", second.id));
    let mut e = reopen(&path, &owner);
    assert!(e
        .decide_request(&first.id, RequestDecision::AlwaysApprove)
        .is_err());
    assert_eq!(e.items().len(), 1);
    assert_eq!(e.outbox_len(), 1);
    assert_eq!(
        e.requests()
            .iter()
            .find(|r| r.id == second.id)
            .unwrap()
            .status,
        RequestStatus::Pending
    );
    drop(e);
    fault(&path, "DROP TRIGGER fail_covered");
    let mut e = reopen(&path, &owner);
    let resumed = e
        .decide_request(&first.id, RequestDecision::AlwaysApprove)
        .unwrap();
    assert_eq!(resumed.also_resolved, 1);
    assert_eq!(e.items().len(), 2);
    assert_eq!(e.outbox_len(), 2);
    let original = e.outbox_envelopes();
    w.start_relay();
    let mut person = RawClient::reconnect(&w.relay_url(), owner.author.clone()).await;
    let first_echo = publish_after_restart(&mut person, original[0].clone()).await;
    // The relay persisted the receipt, but the core never receives this acknowledgement.
    drop(person);
    drop(e);
    w.stop_relay();
    w.start_relay();
    let mut e = reopen(&path, &owner);
    assert_eq!(e.outbox_len(), 2);
    let mut person = RawClient::reconnect(&w.relay_url(), owner.author.clone()).await;
    for env in e.outbox_envelopes() {
        let echo = publish_after_restart(&mut person, env).await;
        if echo.env.client_id() == first_echo.env.client_id() {
            assert_eq!(echo.seq, first_echo.seq);
            assert_eq!(echo.hash, first_echo.hash);
        }
        ingest(&mut e, echo);
    }
    assert_eq!(e.outbox_len(), 0);
    assert_eq!(e.items().len(), 2);
    assert!(e
        .requests()
        .iter()
        .all(|r| r.status == RequestStatus::Approved));
    assert_eq!(e.agent_profile(&first.agent).budget_spent_cents, Some(74));
    assert_eq!(
        e.decide_request(&first.id, RequestDecision::AlwaysApprove)
            .unwrap()
            .also_resolved,
        0
    );
    assert_eq!(e.outbox_len(), 0);
    let events = w.events_in(&space).await;
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.env.body(), Some(EventBody::RequestResolved { .. })))
            .count(),
        2
    );
    assert!(e.verify_all().iter().all(|result| result.valid));
    drop(e);
    let e = reopen(&path, &owner);
    assert_eq!(e.items().len(), 2);
    assert_eq!(e.agent_profile(&first.agent).budget_spent_cents, Some(74));
    assert_eq!(e.outbox_len(), 0);
}

async fn competing_owner_device(w: &World, owner: &Owner, sponsor: &mut RawClient) -> RawClient {
    let author = Author::device(&owner.root, Signer::generate());
    assert!(matches!(
        sponsor
            .request(Op::DeliverLink {
                id: author.device.clone().unwrap(),
                sealed: vec![7],
                device: Some(DeviceCertificate {
                    device: author.device.clone().unwrap(),
                    cert: author.cert.clone().unwrap()
                }),
            })
            .await,
        Ok(Reply::Done)
    ));
    RawClient::reconnect(&w.relay_url(), author).await
}

async fn ordered_change_before_approval_echo(removal: bool) {
    let w = World::new(if removal {
        "approval_removed"
    } else {
        "approval_denied"
    })
    .await;
    let path = w.dir.join("owner.sqlite");
    let (mut e, owner, mut person, mut agent, space) = setup(&w, &path).await;
    let mut other = competing_owner_device(&w, &owner, &mut person).await;
    let request = propose(&mut e, &mut agent, &space, "Conflicting approval").await;
    e.decide_request(&request.id, RequestDecision::AlwaysApprove)
        .unwrap();
    assert_eq!(e.items().len(), 1);
    let queued = e.outbox_envelopes()[0].clone();
    let change = if removal {
        EventBody::MemberRemoved {
            identity: agent.identity(),
        }
    } else {
        EventBody::RequestResolved {
            request: request.id.clone(),
            approved: false,
            content_hash: request.content_hash,
            resolution: Some(RequestResolution {
                message: "Declined on the other owner device".into(),
                created_item: None,
                ai_cost_cents: 37,
                standing_grant: None,
            }),
        }
    };
    let seen = e.causal_head(&space).unwrap();
    ingest(
        &mut e,
        other.publish_body(&space, seen, change).await.unwrap(),
    );
    let status = if removal {
        RequestStatus::Stale
    } else {
        RequestStatus::Denied
    };
    assert_eq!(
        e.requests()
            .iter()
            .find(|r| r.id == request.id)
            .unwrap()
            .status,
        status
    );
    assert!(e.items().is_empty());
    assert!(e.standing_decisions().is_empty());
    ingest(&mut e, person.publish_env(queued).await.unwrap());
    assert_eq!(e.outbox_len(), 0);
    assert_eq!(
        e.requests()
            .iter()
            .find(|r| r.id == request.id)
            .unwrap()
            .status,
        status
    );
    assert!(e.items().is_empty());
    assert_eq!(e.agent_profile(&request.agent).budget_spent_cents, Some(37));
    assert!(e.verify_all().iter().all(|result| result.valid));
    drop(e);
    let e = reopen(&path, &owner);
    assert_eq!(
        e.requests()
            .iter()
            .find(|r| r.id == request.id)
            .unwrap()
            .status,
        status
    );
    assert!(e.items().is_empty());
    assert!(e.standing_decisions().is_empty());
    assert_eq!(e.agent_profile(&request.agent).budget_spent_cents, Some(37));
    let mut observer = Engine::open(":memory:").unwrap();
    let Ok(Reply::Profiles(profiles)) = person
        .request(Op::Profiles {
            ids: vec![owner.author.identity.clone(), request.agent.clone()],
        })
        .await
    else {
        panic!("directory profiles")
    };
    observer.put_profiles(profiles).unwrap();
    for event in w.events_in(&space).await {
        ingest(&mut observer, event);
    }
    assert_eq!(observer.requests()[0].status, status);
    assert!(observer.items().is_empty());
    assert_eq!(
        observer.agent_profile(&request.agent).budget_spent_cents,
        Some(37)
    );
}

#[tokio::test]
async fn the_other_owner_devices_ordered_denial_wins_before_the_approval_echo() {
    ordered_change_before_approval_echo(false).await;
}

#[tokio::test]
async fn the_other_owner_devices_ordered_removal_invalidates_the_approval_before_its_echo() {
    ordered_change_before_approval_echo(true).await;
}
