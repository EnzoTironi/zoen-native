//! Narrow local subprocess protocol. No user-controlled authority/configuration.
use serde::{Deserialize, Serialize};

pub(crate) const MAX_INPUT: usize = 16 * 1024;
pub(crate) const MAX_OUTPUT: usize = 8 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Command {
    PolicyImport {
        agent: String,
        signed: Box<types::owner_budget::SignedOwnerPolicy>,
        policy_digest: String,
    },
    PolicyContext {
        agent: String,
        signed: Box<types::owner_policy_context::SignedOwnerPolicyInquiry>,
    },
    Stage {
        agent: String,
        space: String,
        source: String,
        prompt: String,
    },
    Advance {
        agent: String,
        space: String,
        request: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PolicyProgress {
    /// Only a precise missing-group rejection from the configured native core.
    /// Not a quote/refusal, and never evidence that a Welcome has arrived.
    NeedsWelcome {
        inquiry_digest: String,
        device: String,
    },
    /// A diagnostic outcome only: never a refusal, receipt or retry permission.
    Unavailable {
        operation: PolicyOperation,
        code: PolicyUnavailable,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_stage: Option<zoen_runtime::OwnerPolicyContextStage>,
    },
    Installed {
        policy_digest: String,
    },
    Refused {
        policy_digest: String,
    },
    Options {
        options: Box<types::owner_policy_context::OwnerPolicyOptions>,
    },
    OptionsRefused {
        inquiry_digest: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PolicyOperation {
    Context,
    Import,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PolicyUnavailable {
    InvalidPolicy,
    InvalidProfile,
    DeploymentMismatch,
    InvalidBinding,
    Denied,
    OverBudget,
    CoreAuthorityUnavailable,
    RestoreUnreconciled,
    Unavailable,
    EvidenceTooLarge,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Progress {
    CatchingUp,
    Review {
        request: String,
        content_hash: String,
    },
    Recovery {
        run: String,
    },
    Terminal {
        request: String,
        reason: Terminal,
    },
    Delivered {
        run: String,
        item: String,
        sha256: String,
    },
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Terminal {
    Denied,
    Cancelled,
    Stale,
    Expired,
}

impl Command {
    pub(crate) fn validate(&self) -> Result<(), String> {
        fn locator(s: &str) -> bool {
            !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
        }
        match self {
            Self::PolicyImport {
                agent,
                signed,
                policy_digest,
            } if locator(agent)
                && signed.policy.valid_shape()
                && event_log::owner_budget::digest(&signed.policy).as_ref()
                    == Some(policy_digest)
                && event_log::owner_budget::verify(signed) =>
            {
                Ok(())
            }
            Self::PolicyContext { agent, signed }
                if agent == &signed.inquiry.agent
                    && locator(agent)
                    && event_log::owner_policy_inquiry::verify(signed) =>
            {
                Ok(())
            }
            Self::Stage {
                agent,
                space,
                source,
                prompt,
            } if locator(agent)
                && locator(space)
                && locator(source)
                && !prompt.trim().is_empty()
                && prompt.len() <= 8192 =>
            {
                Ok(())
            }
            Self::Advance {
                agent,
                space,
                request,
            } if locator(agent) && locator(space) && locator(request) => Ok(()),
            _ => Err("invalid image content command".into()),
        }
    }
}
