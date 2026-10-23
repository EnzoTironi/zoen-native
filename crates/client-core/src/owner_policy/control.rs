//! Dedicated non-model controls, extracted only from confirmed signed MLS events.
use super::*;
use types::{owner_budget::SignedOwnerPolicy, Event, OwnerPolicyOutcome};

/// Minted from the actual Core log; never from a prompt, model or IPC argument.
pub struct OwnerPolicySubmission {
    source: String,
    sequence: u64,
    space: String,
    agent: String,
    signed: SignedOwnerPolicy,
    digest: String,
}
impl OwnerPolicySubmission {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn space(&self) -> &str {
        &self.space
    }
    pub fn agent(&self) -> &str {
        &self.agent
    }
    pub fn signed(&self) -> &SignedOwnerPolicy {
        &self.signed
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

pub(super) fn original(event: &Event, agent: &str) -> Option<(SignedOwnerPolicy, String)> {
    let EventBody::OwnerPolicySubmitted {
        agent: target,
        signed,
    } = &event.body
    else {
        return None;
    };
    if target != agent
        || event.seq == 0
        || event.sealed_wire.is_none()
        || event.author != signed.policy.owner
        || event.device.as_deref() != Some(signed.device.as_str())
        || event.cert.as_deref() != Some(signed.cert.as_str())
        || event_log::verify_author(event).is_err()
        || !event_log::owner_budget::verify(signed)
    {
        return None;
    }
    Some((
        signed.clone(),
        event_log::owner_budget::digest(&signed.policy)?,
    ))
}

impl Engine {
    pub(crate) fn owner_policy_status(
        &self,
        scope: &OwnerPolicyScope,
        source: &str,
    ) -> R<OwnerPolicyStatus> {
        self.owner_policy_conversation(scope)?;
        let log = self.logs.get(&scope.space_id).ok_or_else(denied)?;
        let pending = self.store.outbox()?;
        let pending = pending
            .iter()
            .find(|entry| entry.event.client_id == source && entry.event.space == scope.space_id);
        let event = log
            .events()
            .iter()
            .find(|event| event.client_id == source)
            .or_else(|| pending.map(|entry| &entry.event))
            .ok_or_else(denied)?;
        let EventBody::OwnerPolicySubmitted { agent, signed } = &event.body else {
            return Err(denied());
        };
        if agent != &scope.agent_id
            || signed.policy.owner != scope.expected_identity_id
            || event.author != scope.expected_identity_id
            || event.device.as_deref() != Some(signed.device.as_str())
            || event.cert.as_deref() != Some(signed.cert.as_str())
            || event_log::verify_author(event).is_err()
            || !event_log::owner_budget::verify(signed)
        {
            return Err(denied());
        }
        let digest = event_log::owner_budget::digest(&signed.policy).ok_or_else(invalid)?;
        let outcomes = log.events().iter().filter_map(|reply| {
            let EventBody::OwnerPolicyReported {
                source: target,
                policy_digest,
                outcome,
            } = &reply.body
            else {
                return None;
            };
            (target == source
                && policy_digest == &digest
                && reply.author == scope.agent_id
                && reply.seq > event.seq
                && reply.sealed_wire.is_some()
                && reply.device.is_some()
                && reply.cert.is_some()
                && event_log::verify_author(reply).is_ok())
            .then_some(outcome)
        });
        let outcome = outcomes.fold(None, |previous, next| {
            if matches!(previous, Some(OwnerPolicyOutcome::Installed)) {
                previous
            } else {
                Some(next.clone())
            }
        });
        Ok(OwnerPolicyStatus {
            source: source.into(),
            policy_digest: digest,
            state: match outcome {
                Some(OwnerPolicyOutcome::Installed) => "installed",
                Some(OwnerPolicyOutcome::Refused) => "refused",
                None if pending.is_some_and(|entry| entry.failed) => "delivery_failed",
                None => "pending",
            }
            .into(),
        })
    }

    pub(super) fn owner_policy_agent(&self, space: &str) -> R<String> {
        let account = self.account().ok_or_else(denied)?;
        self.native_surface_owner(&account.identity, &account.device)?;
        let agent = self.identities.get(&account.identity).ok_or_else(denied)?;
        let owner = agent.owner.as_deref().ok_or_else(denied)?;
        let state = self.space_state(space)?;
        if !account.registered
            || agent.kind != IdentityKind::Agent
            || !event_log::agent_owner::profile_authorized(agent)
            || state.kind != SpaceKind::Direct
            || state.privacy != Privacy::EndToEnd
            || state.integrity_error.is_some()
            || state.members.len() != 2
            || !self.is_member(space, owner)
            || !self.is_member(space, &agent.id)
            || !self.state.writer(space, &agent.id)
            || self.ready_epoch(space).is_none()
        {
            return Err(denied());
        }
        Ok(agent.id.clone())
    }

    pub(crate) fn owner_policy_submission(
        &self,
        space: &str,
        source: &str,
    ) -> R<OwnerPolicySubmission> {
        let agent_id = self.owner_policy_agent(space)?;
        let agent = self.identities.get(&agent_id).ok_or_else(denied)?;
        let owner = agent.owner.as_deref().ok_or_else(denied)?;
        let event = self
            .logs
            .get(space)
            .and_then(|log| log.events().iter().find(|event| event.client_id == source))
            .ok_or_else(denied)?;
        let (signed, digest) = original(event, &agent.id).ok_or_else(denied)?;
        if signed.policy.owner != owner {
            return Err(denied());
        }
        Ok(OwnerPolicySubmission {
            source: source.into(),
            sequence: event.seq,
            space: space.into(),
            agent: agent.id.clone(),
            signed,
            digest,
        })
    }
}

impl Client {
    /// Bounded typed discovery; use before normal message turns advance a cursor.
    pub fn owner_policy_submissions(
        &self,
        space: &str,
        after_seq: u64,
        limit: u32,
    ) -> R<Vec<OwnerPolicySubmission>> {
        if !(1..=50).contains(&limit) {
            return Err(invalid());
        }
        let engine = self.lock();
        engine.owner_policy_agent(space)?;
        let mut controls = Vec::new();
        for event in engine.logs.get(space).ok_or_else(denied)?.events() {
            if event.seq <= after_seq
                || !matches!(event.body, EventBody::OwnerPolicySubmitted { .. })
            {
                continue;
            }
            match engine.owner_policy_submission(space, &event.client_id) {
                Ok(control) => {
                    if !engine.owner_policy_report_exists(&control, None)? {
                        controls.push(control);
                    }
                }
                Err(CoreError::Forbidden { .. }) => continue,
                Err(error) => return Err(error),
            }
            if controls.len() == limit as usize {
                break;
            }
        }
        Ok(controls)
    }
    /// Re-run immediately before importing. Runtime still checks live SQL authority.
    pub fn owner_policy_submission(&self, space: &str, source: &str) -> R<OwnerPolicySubmission> {
        self.lock().owner_policy_submission(space, source)
    }
    /// Append only an explicit Runtime outcome for an actual typed original.
    /// Unavailable or ambiguous IPC responses must stay pending; do not call this.
    pub fn report_owner_policy(
        &self,
        submission: &OwnerPolicySubmission,
        installed_digest: Option<&str>,
    ) -> R<()> {
        self.lock()
            .report_owner_policy(submission, installed_digest)
    }
}

impl Engine {
    fn owner_policy_report_exists(
        &self,
        submission: &OwnerPolicySubmission,
        outcome: Option<&OwnerPolicyOutcome>,
    ) -> R<bool> {
        let pending = self.store.outbox()?;
        Ok(pending.iter().map(|entry| &entry.event)
            .chain(self.logs.get(submission.space()).into_iter().flat_map(|log| log.events()))
            .any(|event| {
                matches!(&event.body, EventBody::OwnerPolicyReported { source, policy_digest, outcome: previous }
                    if event.space == submission.space() && event.author == submission.agent()
                    && source == submission.source() && policy_digest == submission.digest()
                    && outcome.is_none_or(|expected| previous == expected || previous == &OwnerPolicyOutcome::Installed)
                    && event.device.is_some() && event.cert.is_some()
                    && event_log::verify_author(event).is_ok())
            }))
    }
    pub(crate) fn report_owner_policy(
        &mut self,
        submission: &OwnerPolicySubmission,
        installed_digest: Option<&str>,
    ) -> R<()> {
        let current = self.owner_policy_submission(submission.space(), submission.source())?;
        if current.digest() != submission.digest()
            || current.agent() != submission.agent()
            || installed_digest.is_some_and(|digest| digest != current.digest())
        {
            return Err(denied());
        }
        let outcome = if installed_digest.is_some() {
            OwnerPolicyOutcome::Installed
        } else {
            OwnerPolicyOutcome::Refused
        };
        if self.owner_policy_report_exists(&current, Some(&outcome))? {
            return Ok(());
        }
        let author = self
            .net
            .account_author(current.agent())
            .ok_or_else(denied)?;
        self.append_owner_policy_report(
            current.space(),
            &author,
            EventBody::OwnerPolicyReported {
                source: current.source().into(),
                policy_digest: current.digest().into(),
                outcome,
            },
        )?;
        Ok(())
    }
}
