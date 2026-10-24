use super::*;
use event_log::{Author, Signer as NativeSigner};
use openmls_traits::{signatures::SignerError, types::SignatureScheme};
use types::{ItemProposal, PlanDoc, PlanLine, PlanSection};

#[derive(Clone)]
struct HostSigner(NativeSigner);
impl Signer for HostSigner {
    fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, SignerError> {
        hex::decode(self.0.sign(bytes)).map_err(|_| SignerError::SigningError)
    }
    fn signature_scheme(&self) -> SignatureScheme {
        mls::SUITE.signature_algorithm()
    }
}
struct Fixture {
    client: Client<HostSigner>,
    owner: Author,
    agent: Author,
    stranger: Author,
    now: i64,
}
impl Fixture {
    fn new() -> Self {
        let identity = NativeSigner::generate();
        let device = NativeSigner::generate();
        let owner = Author::device(&identity, NativeSigner::generate());
        let agent_key = NativeSigner::generate();
        let agent = Author::root(agent_key);
        let stranger = Author::root(NativeSigner::generate());
        let account = LinkedAccount {
            identity: identity.id(),
            device: device.id(),
            cert: identity.sign(&event_log::device_cert_message(&device.id())),
            name: "Owner".into(),
            handle: "owner".into(),
            relay: "http://localhost".into(),
            primary: identity.id(),
            primary_hpke: String::new(),
            hpke_secret: String::new(),
            link_secret: String::new(),
            agreement_secret: String::new(),
            profile_key: String::new(),
            profile_key_version: 0,
            profile_seen_version: 0,
            profile_fields: String::new(),
        };
        let mut client = Client::new(account, HostSigner(device), "").unwrap();
        client.synced = true;
        let profile = Identity {
            id: agent.identity.clone(),
            kind: IdentityKind::Agent,
            name: "Agent".into(),
            handle: "agent".into(),
            tint_hex: "#123456".into(),
            glyph: None,
            owner: Some(identity.id()),
            bio: String::new(),
            owner_proof: Some(Box::new(
                event_log::agent_owner::authorize(&owner, &agent.identity).unwrap(),
            )),
        };
        client.saved.profiles.insert(profile.id.clone(), profile);
        let mut fixture = Self {
            client,
            owner,
            agent,
            stranger,
            now: 1_791_200_000_000,
        };
        for (space, kind) in [
            ("sp_personal", SpaceKind::Personal),
            ("sp_chat", SpaceKind::Group),
        ] {
            fixture.append(
                space,
                "owner",
                EventBody::SpaceCreated {
                    title: space.into(),
                    kind,
                    privacy: Privacy::Closed,
                },
            );
        }
        fixture.append(
            "sp_chat",
            "owner",
            EventBody::MemberAdded {
                identity: fixture.agent.identity.clone(),
                role: Role::Member,
            },
        );
        fixture.append(
            "sp_chat",
            "owner",
            EventBody::MemberAdded {
                identity: fixture.stranger.identity.clone(),
                role: Role::Member,
            },
        );
        fixture.grant(
            "sp_personal",
            "budget",
            GrantScope::Everywhere,
            Capability::MonthlyBudget {
                cents: 9_007_199_254_740_993,
            },
        );
        fixture.grant(
            "sp_chat",
            "trust",
            GrantScope::Space("sp_chat".into()),
            Capability::Trust(TrustLevel::Suggest),
        );
        fixture
    }
    fn append(&mut self, space: &str, who: &str, body: EventBody) {
        self.now += 1;
        let author = match who {
            "agent" => &self.agent,
            "stranger" => &self.stranger,
            _ => &self.owner,
        };
        let log = self
            .client
            .logs
            .entry(space.into())
            .or_insert_with(|| SpaceLog::new(space));
        let event = author.sign_event(
            space,
            &types::new_ulid(self.now),
            self.now,
            log.head(),
            body,
        );
        log.sequence(event);
    }
    fn grant(&mut self, space: &str, id: &str, scope: GrantScope, capability: Capability) {
        self.append(
            space,
            "owner",
            EventBody::GrantIssued {
                grant: Grant {
                    id: id.into(),
                    grantor: self.owner.identity.clone(),
                    grantee: Some(self.agent.identity.clone()),
                    scope,
                    capability,
                    expires_at_ms: None,
                },
            },
        );
    }
    fn command(&mut self, command: Value) -> Result<documents::DocumentResponse, String> {
        self.now += 1;
        self.client.agent_request(&command.to_string(), self.now)
    }
    fn proposal(&mut self) -> AgentRequest {
        let proposal = ItemProposal {
            item: "it_proposal".into(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(PlanDoc {
                title: "Retained".into(),
                summary: String::new(),
                budget_cents: None,
                sections: vec![PlanSection {
                    title: "Actions".into(),
                    lines: vec![PlanLine {
                        id: "ln_one".into(),
                        text: "Exact output".into(),
                        cost_cents: 9_007_199_254_740_993,
                        done: false,
                    }],
                }],
            }),
            origin: "Agent".into(),
            message: "A proposta foi compartilhada.".into(),
            ai_cost_cents: 37,
        };
        let mut request = AgentRequest {
            tool_proposal: None,
            id: "rq_proposal".into(),
            agent: self.agent.identity.clone(),
            title: "Create plan".into(),
            detail: "Retained output".into(),
            audience: "This chat".into(),
            action: ActionClass::Reversible,
            content_hash: String::new(),
            item: None,
            line: None,
            proposal: Some(Box::new(proposal)),
        };
        request.content_hash = request.proposal_hash("sp_chat").unwrap();
        request
    }
}

#[test]
fn exact_budgets_ignore_another_members_forged_grant_and_reject_overflow_without_writes() {
    let mut fixture = Fixture::new();
    let id = fixture.agent.identity.clone();
    let profile = fixture
        .command(json!({"op":"agent_profile","agent_id":id}))
        .unwrap()
        .result;
    assert_eq!(profile["budget_limit_cents"], "9007199254740993");
    fixture.append(
        "sp_chat",
        "stranger",
        EventBody::GrantIssued {
            grant: Grant {
                id: "gr_forged".into(),
                grantor: fixture.stranger.identity.clone(),
                grantee: Some(id.clone()),
                scope: GrantScope::Space("sp_chat".into()),
                capability: Capability::Trust(TrustLevel::Autonomous),
                expires_at_ms: None,
            },
        },
    );
    let state = fixture.client.agent_projection().unwrap();
    assert_eq!(
        fixture
            .client
            .agent_trust(&state, &id, "sp_chat", fixture.now),
        TrustLevel::Suggest
    );
    let before = fixture.client.saved.outbox.len();
    assert!(fixture
        .command(json!({"op":"raise_budget","agent_id":id,"extra_cents":"9223372036854775807"}))
        .is_err());
    assert_eq!(fixture.client.saved.outbox.len(), before);
}
#[test]
fn retained_proposal_approval_creates_the_exact_item_and_charges_usage_only_once_after_reload() {
    let mut fixture = Fixture::new();
    let request = fixture.proposal();
    fixture.append(
        "sp_chat",
        "agent",
        EventBody::RequestOpened {
            request: request.clone(),
        },
    );
    let decision = fixture
        .command(json!({"op":"decide_request","request_id":request.id,"decision":"AlwaysApprove"}))
        .unwrap();
    assert_eq!(decision.result["request"]["status"], "Approved");
    assert_eq!(
        decision.result["standing_grant_id"],
        "gr_standing_rq_proposal"
    );
    let count = fixture.client.saved.outbox.len();
    let retry = fixture
        .command(json!({"op":"decide_request","request_id":request.id,"decision":"AlwaysApprove"}))
        .unwrap();
    assert_eq!(fixture.client.saved.outbox.len(), count);
    let snapshot = retry.update.unwrap().snapshot;
    let mut reopened = Client::new(
        fixture.client.account.clone(),
        fixture.client.signer.clone(),
        &snapshot,
    )
    .unwrap();
    let profile = reopened
        .agent_request(
            &json!({"op":"agent_profile","agent_id":fixture.agent.identity}).to_string(),
            fixture.now,
        )
        .unwrap()
        .result;
    assert_eq!(profile["budget_spent_cents"], "37");
    let item = reopened
        .document_request(
            &json!({"op":"item","item_id":"it_proposal"}).to_string(),
            fixture.now,
        )
        .unwrap()
        .result;
    assert_eq!(
        item["plan"]["sections"][0]["lines"][0]["cost_cents"],
        "9007199254740993"
    );
    assert_eq!(item["created_by"]["id"], fixture.agent.identity);
}
#[test]
fn red_lines_cannot_be_always_approved_and_another_member_cannot_resolve_a_request() {
    let mut fixture = Fixture::new();
    let request = AgentRequest {
        tool_proposal: None,
        id: "rq_money".into(),
        agent: fixture.agent.identity.clone(),
        title: "Book".into(),
        detail: "Hotel".into(),
        audience: "Group".into(),
        action: ActionClass::Money { cents: 64000 },
        content_hash: "exact".into(),
        item: None,
        line: None,
        proposal: None,
    };
    fixture.append(
        "sp_chat",
        "agent",
        EventBody::RequestOpened {
            request: request.clone(),
        },
    );
    fixture.append(
        "sp_chat",
        "stranger",
        EventBody::RequestResolved {
            request: request.id.clone(),
            approved: true,
            content_hash: request.content_hash.clone(),
            resolution: None,
        },
    );
    let read = fixture.command(json!({"op":"requests"})).unwrap().result;
    assert_eq!(read[0]["status"], "Pending");
    assert_eq!(read[0]["can_always_approve"], false);
    let before = fixture.client.saved.outbox.len();
    assert!(fixture
        .command(json!({"op":"decide_request","request_id":request.id,"decision":"AlwaysApprove"}))
        .is_err());
    assert_eq!(fixture.client.saved.outbox.len(), before);
    let accepted = fixture
        .command(json!({"op":"decide_request","request_id":request.id,"decision":"Approve"}))
        .unwrap()
        .result;
    assert_eq!(accepted["request"]["status"], "Approved");
    assert!(accepted["message"].as_str().unwrap().contains("Aguardando"));
}
#[test]
fn editing_a_line_invalidates_the_exact_content_approval() {
    let mut fixture = Fixture::new();
    let request = fixture.proposal();
    let proposal = request.proposal.unwrap();
    let ItemContent::Plan(mut plan) = proposal.content else {
        panic!()
    };
    fixture.append(
        "sp_chat",
        "owner",
        EventBody::ItemCreated {
            item: proposal.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(plan.clone()),
            origin: "Fixture".into(),
        },
    );
    let line = &plan.sections[0].lines[0];
    let request = AgentRequest {
        tool_proposal: None,
        id: "rq_line".into(),
        agent: fixture.agent.identity.clone(),
        title: "Execute".into(),
        detail: line.text.clone(),
        audience: "Group".into(),
        action: ActionClass::Money {
            cents: line.cost_cents,
        },
        content_hash: event_log::content_hash(&(&line.text, line.cost_cents)),
        item: Some(proposal.item.clone()),
        line: Some(line.id.clone()),
        proposal: None,
    };
    fixture.append(
        "sp_chat",
        "agent",
        EventBody::RequestOpened {
            request: request.clone(),
        },
    );
    plan.sections[0].lines[0].text = "Edited".into();
    fixture.append(
        "sp_chat",
        "owner",
        EventBody::ItemVersioned {
            item: proposal.item,
            content: ItemContent::Plan(plan),
            note: "Changed".into(),
        },
    );
    assert_eq!(
        fixture.command(json!({"op":"requests"})).unwrap().result[0]["status"],
        "Stale"
    );
    assert!(fixture
        .command(json!({"op":"decide_request","request_id":request.id,"decision":"Approve"}))
        .is_err());
    assert!(fixture
        .command(json!({"op":"decide_request","request_id":request.id,"decision":"Deny"}))
        .is_ok());
}

#[test]
fn revocation_controls_cannot_revoke_another_kind_of_grant() {
    let mut fixture = Fixture::new();
    let request = fixture.proposal();
    fixture.append(
        "sp_chat",
        "agent",
        EventBody::RequestOpened {
            request: request.clone(),
        },
    );
    fixture
        .command(json!({"op":"decide_request","request_id":request.id,"decision":"AlwaysApprove"}))
        .unwrap();
    let count = fixture.client.saved.outbox.len();
    assert!(fixture
        .command(json!({"op":"revoke_app_device","grant_id":"gr_standing_rq_proposal"}))
        .is_err());
    assert_eq!(fixture.client.saved.outbox.len(), count);
    let installed = fixture
        .client
        .app_request(
            &json!({"op":"install_app","space_id":"sp_chat","app_id":"list","args_json":"{}"})
                .to_string(),
            fixture.now + 1,
            "pt-BR",
        )
        .unwrap()
        .result;
    let item = installed["id"].as_str().unwrap();
    let grant = fixture
        .client
        .grant_browser_app_device(
            item,
            "photos.pick",
            "Choose only these photos",
            false,
            fixture.now + 2,
        )
        .unwrap();
    let count = fixture.client.saved.outbox.len();
    assert!(fixture
        .command(json!({"op":"revoke_standing","grant_id":grant}))
        .is_err());
    assert_eq!(fixture.client.saved.outbox.len(), count);
    fixture
        .command(json!({"op":"revoke_app_device","grant_id":grant}))
        .unwrap();
    assert!(!fixture
        .client
        .active_app_grants(item, fixture.now)
        .unwrap()
        .iter()
        .any(|value| value.id == grant));
}
