use crate::dto::*;
use crate::engine::{Engine, ReqStatus, RequestState};
use types::*;

struct Chat {
    path: String,
    owner: String,
    agent: String,
    space: String,
}

impl Chat {
    fn create() -> std::result::Result<(Self, Engine), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir()
            .join(format!("{}.sqlite", new_id("zoen_approval")?))
            .to_string_lossy()
            .into_owned();
        let mut e = Engine::open(&path).unwrap();
        let owner = e
            .create_identity(identity(IdentityKind::Person, None), true)
            .unwrap();
        e.me = Some(owner.clone());
        e.store.set_meta("me", &owner).unwrap();
        let agent = e
            .create_identity(identity(IdentityKind::Agent, Some(owner.clone())), true)
            .unwrap();
        let author = event_log::Author::device(&e.signers[&owner], event_log::Signer::generate()?);
        let profile = e.identities.get_mut(&agent).unwrap();
        profile.owner_proof = Some(Box::new(
            event_log::agent_owner::authorize(&author, &agent).unwrap(),
        ));
        e.store
            .put_identity(profile, Some(&e.signers[&agent].secret()))
            .unwrap();
        let space = new_id("sp")?;
        e.append(
            &space,
            &owner,
            EventBody::SpaceCreated {
                title: "Approval journey".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )
        .unwrap();
        e.append(
            &space,
            &owner,
            EventBody::MemberAdded {
                identity: agent.clone(),
                role: Role::Member,
            },
        )
        .unwrap();
        e.set_trust(&agent, &space, TrustLevel::Suggest).unwrap();
        e.append(
            &space,
            &owner,
            EventBody::GrantIssued {
                grant: Grant {
                    id: new_id("gr")?,
                    grantor: owner.clone(),
                    grantee: Some(agent.clone()),
                    scope: GrantScope::Everywhere,
                    capability: Capability::MonthlyBudget { cents: 1000 },
                    expires_at_ms: None,
                },
            },
        )
        .unwrap();
        Ok((
            Self {
                path,
                owner,
                agent,
                space,
            },
            e,
        ))
    }

    fn reopen(&self) -> Engine {
        Engine::open(&self.path).unwrap()
    }
}

impl Drop for Chat {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.path));
        }
    }
}

fn identity(kind: IdentityKind, owner: Option<String>) -> Identity {
    Identity {
        owner_proof: None,
        id: String::new(),
        kind,
        name: "Journey principal".into(),
        handle: String::new(),
        tint_hex: "#4F7CFF".into(),
        glyph: None,
        owner,
        bio: String::new(),
    }
}

fn plan() -> PlanDto {
    PlanDto {
        title: "Persist this exact plan".into(),
        summary: "Owner review".into(),
        budget_cents: Some(5000),
        total_cents: 0,
        sections: vec![PlanSectionDto {
            title: "Work".into(),
            lines: vec![PlanLineDto {
                id: "line_original".into(),
                text: "A durable proposal".into(),
                cost_cents: 4200,
                done: false,
            }],
        }],
    }
}

fn ingest_signed(
    e: &mut Engine,
    space: &str,
    author: &str,
    body: EventBody,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut log = event_log::SpaceLog::from_events(space, e.logs[space].events().to_vec()).unwrap();
    let signer = e.author_for(author).unwrap();
    let signed = signer.sign_event(
        space,
        &new_ulid(crate::engine::now_ms())?,
        crate::engine::now_ms(),
        log.head(),
        body,
    );
    let event = log.sequence(signed).clone();
    let frame = protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: protocol::Envelope::plain(&event),
    };
    assert_eq!(e.ingest(frame), crate::sync::Ingest::Applied);

    Ok(())
}

fn resolution(r: &RequestState) -> EventBody {
    let p = r.req.proposal.as_ref().unwrap();
    EventBody::RequestResolved {
        request: r.req.id.clone(),
        approved: true,
        content_hash: r.req.content_hash.clone(),
        resolution: Some(RequestResolution {
            tool_decision: None,
            message: p.message.clone(),
            created_item: Some(p.item.clone()),
            ai_cost_cents: p.ai_cost_cents,
            standing_grant: None,
        }),
    }
}

#[test]
fn authenticated_wrong_owner_hash_space_and_duplicate_decisions_have_no_effect_on_ingest_or_replay()
{
    authenticity::authenticated_wrong_owner_hash_space_and_duplicate_decisions_have_no_effect_on_ingest_or_replay().unwrap()
}

#[test]
fn changed_proposals_impersonated_requests_and_forged_usage_are_not_projected() {
    authenticity::changed_proposals_impersonated_requests_and_forged_usage_are_not_projected()
        .unwrap()
}

#[test]
fn always_approve_commits_its_grant_with_the_item_and_resumes_an_interrupted_batch() {
    execution::always_approve_commits_its_grant_with_the_item_and_resumes_an_interrupted_batch()
        .unwrap()
}

#[test]
fn agent_removal_prevents_execution_and_denial_keeps_the_model_charge_once() {
    execution::agent_removal_prevents_execution_and_denial_keeps_the_model_charge_once().unwrap()
}

#[test]
fn direct_agent_outputs_and_usage_roll_back_together_when_a_later_write_fails() {
    execution::direct_agent_outputs_and_usage_roll_back_together_when_a_later_write_fails().unwrap()
}

#[test]
fn a_second_device_projects_the_same_output_after_its_agent_profile_arrives() {
    recovery::a_second_device_projects_the_same_output_after_its_agent_profile_arrives().unwrap()
}

#[test]
fn revoked_standing_grants_cannot_resume_an_interrupted_batch() {
    execution::revoked_standing_grants_cannot_resume_an_interrupted_batch().unwrap()
}

#[test]
fn claiming_an_owner_without_their_signed_grant_cannot_charge_for_a_proposal() {
    authenticity::claiming_an_owner_without_their_signed_grant_cannot_charge_for_a_proposal()
        .unwrap()
}

#[test]
fn an_approval_cannot_redirect_its_output_and_later_events_cannot_overwrite_or_edit_it_from_another_space(
) {
    authenticity::an_approval_cannot_redirect_its_output_and_later_events_cannot_overwrite_or_edit_it_from_another_space().unwrap()
}

#[test]
fn plan_approval_after_restart_creates_the_exact_item_and_debits_usage_once() {
    recovery::plan_approval_after_restart_creates_the_exact_item_and_debits_usage_once().unwrap()
}

#[test]
fn app_approval_after_restart_creates_an_usable_item_with_its_scoped_grant() {
    recovery::app_approval_after_restart_creates_an_usable_item_with_its_scoped_grant().unwrap()
}

#[test]
fn failed_approval_write_leaves_the_proposal_pending_and_identical_retry_succeeds() {
    recovery::failed_approval_write_leaves_the_proposal_pending_and_identical_retry_succeeds()
        .unwrap()
}

mod encrypted;
mod review;

mod authenticity;
mod execution;
mod recovery;
