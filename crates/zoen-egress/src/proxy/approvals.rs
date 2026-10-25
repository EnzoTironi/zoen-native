//! Approval preparation precedes mutation of pending cards and network authority.
use super::{
    host_matches, ApprovalScope, Decision, Egress, EgressRule, LeasePolicy, Pending, State, Target,
};
use sha2::{Digest, Sha256};
use types::{ActionClass, AgentRequest, Capability, Grant, GrantScope, IdError};

impl Egress {
    /// The owner said yes. `AlwaysForTool` returns the exact Grant to retain.
    ///
    /// # Errors
    /// Unavailable state or ID preparation leaves the pending decision unchanged.
    pub fn approve(&self, request: &str, scope: ApprovalScope) -> Result<Option<Grant>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Egress approval state is poisoned".to_owned())?;
        approve_in(&mut state, request, scope, || types::new_id("grant"))
    }
    pub(super) fn open_card(
        &self,
        policy: &LeasePolicy,
        target: &Target,
        method: &str,
    ) -> Result<String, Decision> {
        let mut st = self.state.lock().map_err(|_| Decision::UpstreamError)?;
        if let Some((id, _)) = st.pending.iter().find(|(_, p)| {
            p.lease == policy.lease
                && host_matches(&p.host, &target.host)
                && p.port == target.port
                && p.method.eq_ignore_ascii_case(method)
        }) {
            return Ok(id.clone());
        }
        let id = types::new_id("req").map_err(|_| Decision::UpstreamError)?;
        let hash = hex::encode(Sha256::digest(
            format!(
                "egress/1|{}|{}|{}|{}|{}",
                policy.lease, policy.tool, target.host, target.port, method
            )
            .as_bytes(),
        ));
        let request = AgentRequest {
            tool_proposal: None,
            id: id.clone(),
            agent: policy.agent.clone(),
            title: format!("Permitir acesso a {}", target.host),
            detail: format!(
                "A ferramenta {} quer se conectar a {} (porta {}), que não está na lista dela.",
                policy.tool, target.host, target.port
            ),
            audience: target.host.clone(),
            action: ActionClass::External,
            content_hash: hash,
            item: None,
            line: None,
            proposal: None,
        };
        st.pending.insert(
            id.clone(),
            Pending {
                lease: policy.lease.clone(),
                owner: policy.owner.clone(),
                agent: policy.agent.clone(),
                tool: policy.tool.clone(),
                host: target.host.clone(),
                port: target.port,
                method: method.to_string(),
            },
        );
        drop(st);
        self.approvals.opened(&policy.lease, &request);
        Ok(id)
    }
}

fn approve_in(
    state: &mut State,
    request: &str,
    scope: ApprovalScope,
    generate: impl FnOnce() -> Result<String, IdError>,
) -> Result<Option<Grant>, String> {
    let Some(pending) = state.pending.get(request) else {
        return Ok(None);
    };
    let rule = EgressRule {
        host: pending.host.clone(),
        ports: vec![pending.port],
        methods: vec![],
    };
    let grant = if scope == ApprovalScope::AlwaysForTool {
        Some(Grant {
            id: generate().map_err(|error| error.to_string())?,
            grantor: pending.owner.clone(),
            grantee: Some(pending.agent.clone()),
            scope: GrantScope::Everywhere,
            capability: Capability::Device {
                capability: format!("net:{}", pending.host),
                purpose: format!("tool {}", pending.tool),
            },
            expires_at_ms: None,
        })
    } else {
        None
    };
    // ID generation is complete before either the pending card or allowlist changes.
    let pending = state
        .pending
        .remove(request)
        .ok_or_else(|| "Pending egress decision disappeared".to_owned())?;
    match scope {
        ApprovalScope::Once => {
            if let Some(lease) = state.leases.get_mut(&pending.lease) {
                lease.once.push(rule);
            }
        }
        ApprovalScope::Task => {
            if let Some(lease) = state.leases.get_mut(&pending.lease) {
                lease.policy.rules.push(rule);
            }
        }
        ApprovalScope::AlwaysForTool => {
            state
                .tool_wide
                .entry((pending.owner, pending.tool))
                .or_default()
                .push(rule);
        }
    }
    Ok(grant)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entropy_failure_cannot_consume_a_card_or_widen_network_access() {
        let mut state = State::default();
        state.pending.insert(
            "request".into(),
            Pending {
                lease: "lease".into(),
                owner: "owner".into(),
                agent: "agent".into(),
                tool: "tool".into(),
                host: "example.invalid".into(),
                port: 443,
                method: "GET".into(),
            },
        );
        let result = approve_in(&mut state, "request", ApprovalScope::AlwaysForTool, || {
            Err(IdError::Entropy(getrandom::Error::UNSUPPORTED))
        });
        assert!(result.is_err());
        assert!(state.pending.contains_key("request"));
        assert!(state.tool_wide.is_empty());
        assert!(state.leases.is_empty());
        let grant = approve_in(&mut state, "request", ApprovalScope::AlwaysForTool, || {
            Ok("prepared-grant".into())
        })
        .unwrap()
        .unwrap();
        assert_eq!(grant.id, "prepared-grant");
        assert_eq!(grant.grantor, "owner");
        assert_eq!(grant.grantee.as_deref(), Some("agent"));
        assert!(state.pending.is_empty());
        assert_eq!(state.tool_wide[&("owner".into(), "tool".into())].len(), 1);
    }
}
