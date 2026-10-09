//! Picks the tier a tool runs in. The lowest tier that satisfies the signed manifest, and
//! a heavier tier only with the owner's Grant; otherwise an approval card.

use crate::manifest::{ManifestError, Needs, SignedManifest, ToolManifest};
use crate::sandbox::SandboxSpec;
use ed25519_dalek::VerifyingKey;
use roda_types::{ActionClass, AgentRequest, Capability, Grant, GrantScope, IdentityId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Cheapest first; the order is the escalation order.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    None,
    Wasm,
    MicroVm,
    Browser,
}

impl Tier {
    pub fn of(needs: &Needs) -> Tier {
        match needs {
            Needs::None => Tier::None,
            Needs::Wasm => Tier::Wasm,
            Needs::MicroVm { .. } => Tier::MicroVm,
            Needs::Browser { .. } => Tier::Browser,
        }
    }

    /// Heavy tiers need a Grant; T0 never does.
    pub fn needs_grant(self) -> bool {
        self >= Tier::MicroVm
    }

    /// The Grant capability that lets `tool` run in this tier (`sandbox:microvm:python`).
    pub fn capability(self, tool: &str) -> String {
        let t = match self {
            Tier::None => "none",
            Tier::Wasm => "wasm",
            Tier::MicroVm => "microvm",
            Tier::Browser => "browser",
        };
        format!("sandbox:{t}:{tool}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// Run now. `spec` is `Some` for T1/T2: the sandbox to start when (and only when) the
    /// first call needs it.
    Run {
        tier: Tier,
        spec: Option<SandboxSpec>,
    },
    /// The owner has to approve first; post this card.
    NeedsApproval(AgentRequest),
    Refused(RouteError),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("the tool asked for {asked:?} but its manifest declares {declared:?}")]
    NotDeclared { declared: Tier, asked: Tier },
}

pub struct Router {
    trusted: Vec<VerifyingKey>,
}

impl Router {
    pub fn new(trusted_publishers: Vec<VerifyingKey>) -> Self {
        Router {
            trusted: trusted_publishers,
        }
    }

    pub fn route(
        &self,
        owner: &str,
        agent: &str,
        signed: &SignedManifest,
        grants: &[Grant],
        now_ms: i64,
    ) -> Route {
        let m = match signed.verify(&self.trusted) {
            Ok(m) => m,
            Err(e) => return Route::Refused(e.into()),
        };
        let tier = Tier::of(&m.needs);
        if !tier.needs_grant() {
            return Route::Run { tier, spec: None };
        }
        let cap = tier.capability(&m.id);
        let granted = grants.iter().any(|g| {
            g.grantor == owner
                && g.grantee.as_deref() == Some(agent)
                && g.expires_at_ms.is_none_or(|t| t > now_ms)
                && matches!(&g.capability, Capability::Device { capability, .. } if *capability == cap)
        });
        if granted {
            Route::Run {
                tier,
                spec: Some(spec_for(m)),
            }
        } else {
            Route::NeedsApproval(card(agent, m))
        }
    }

    /// A call at runtime asking for `asked` (e.g. a model's tool call) can't go above what
    /// the manifest declared.
    pub fn check_call(&self, signed: &SignedManifest, asked: Tier) -> Result<Tier, RouteError> {
        let m = signed.verify(&self.trusted)?;
        let declared = Tier::of(&m.needs);
        if asked > declared {
            return Err(RouteError::NotDeclared { declared, asked });
        }
        Ok(declared)
    }

    /// The Grant the app issues when the owner approves the card from [`Route::NeedsApproval`].
    pub fn grant_for(
        owner: &IdentityId,
        agent: &IdentityId,
        m: &ToolManifest,
        expires_at_ms: Option<i64>,
    ) -> Grant {
        Grant {
            id: roda_types::new_id("grant"),
            grantor: owner.clone(),
            grantee: Some(agent.clone()),
            scope: GrantScope::Everywhere,
            capability: Capability::Device {
                capability: Tier::of(&m.needs).capability(&m.id),
                purpose: m.summary.clone(),
            },
            expires_at_ms,
        }
    }
}

pub fn spec_for(m: &ToolManifest) -> SandboxSpec {
    let (tier, vcpu, mem_mib, disk_mib, max_secs) = match m.needs {
        Needs::MicroVm {
            vcpu,
            mem_mib,
            disk_mib,
            max_secs,
        } => (Tier::MicroVm, vcpu, mem_mib, disk_mib, max_secs),
        Needs::Browser { mem_mib, max_secs } => (Tier::Browser, 2, mem_mib, 4096, max_secs),
        _ => (Tier::of(&m.needs), 0, 0, 0, 0),
    };
    SandboxSpec {
        tier,
        tool: m.id.clone(),
        template: if tier == Tier::Browser {
            "browser".into()
        } else {
            "base".into()
        },
        vcpu,
        mem_mib,
        disk_mib,
        max_secs,
        egress: m.egress.clone(),
        secrets: m.secrets.clone(),
        agent: None,
    }
}

fn card(agent: &str, m: &ToolManifest) -> AgentRequest {
    let tier = Tier::of(&m.needs);
    let (title, mins) = match m.needs {
        Needs::Browser { max_secs, .. } => (
            format!("Abrir um navegador para “{}”", m.summary),
            max_secs.div_ceil(60),
        ),
        Needs::MicroVm { max_secs, .. } => (
            format!("Usar um computador isolado para “{}”", m.summary),
            max_secs.div_ceil(60),
        ),
        _ => (m.summary.clone(), 0),
    };
    let sites = if m.egress.is_empty() {
        "sem acesso à internet".to_string()
    } else {
        format!(
            "acesso só a {}",
            m.egress
                .iter()
                .map(|r| r.host.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let hash = hex::encode(Sha256::digest(
        serde_json::to_vec(m).expect("manifest serializes"),
    ));
    AgentRequest {
        id: roda_types::new_id("req"),
        agent: agent.into(),
        title,
        detail: format!("Até {mins} min por vez, {sites}. Ele é apagado quando a tarefa acaba."),
        audience: m.id.clone(),
        action: if m.egress.is_empty() {
            ActionClass::Reversible
        } else {
            ActionClass::External
        },
        content_hash: format!("{}:{hash}", tier.capability(&m.id)),
        item: None,
        line: None,
    }
}
