//! Adapter for the normal owner-control companion, never a content/model tool.
use crate::{config::Config, control, owner_policy_protocol::PolicyProgress};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    PolicyImport {
        agent: String,
        signed: Box<types::owner_budget::SignedOwnerPolicy>,
        policy_digest: String,
    },
    PolicyContext {
        agent: String,
        signed: Box<types::owner_policy_context::SignedOwnerPolicyInquiry>,
    },
}
pub(super) fn execute(config: &Config, bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > crate::protocol::MAX_INPUT {
        return Err("owner control too large".into());
    }
    let input: Input = serde_json::from_slice(bytes).map_err(|_| "invalid owner control")?;
    let (command, digest) = match input {
        Input::PolicyImport {
            agent,
            signed,
            policy_digest,
        } => {
            if agent != config.agent
                || !event_log::owner_budget::verify(&signed)
                || event_log::owner_budget::digest(&signed.policy).as_ref() != Some(&policy_digest)
            {
                return Err("owner control differs".into());
            }
            (
                json!({"command":"policy_import","signed":signed}),
                Some(policy_digest),
            )
        }
        Input::PolicyContext { agent, signed } => {
            if agent != config.agent
                || agent != signed.inquiry.agent
                || !event_log::owner_policy_inquiry::verify(&signed)
            {
                return Err("owner control differs".into());
            }
            (json!({"command":"policy_context","signed":signed}), None)
        }
    };
    let reply = control::request(
        &config.home,
        &serde_json::to_vec(&command).map_err(|_| "invalid owner control")?,
    )?;
    if let Some(digest) = digest {
        if reply["digest"] != digest {
            return Err("owner control response differs".into());
        }
        let progress = match reply["status"].as_str() {
            Some("policy_installed") => PolicyProgress::Installed {
                policy_digest: digest,
            },
            Some("policy_refused") => PolicyProgress::Refused {
                policy_digest: digest,
            },
            _ => return Err("owner control response differs".into()),
        };
        serde_json::to_value(progress).map_err(|_| "invalid owner control response".into())
    } else {
        let _: PolicyProgress =
            serde_json::from_value(reply.clone()).map_err(|_| "invalid owner control response")?;
        Ok(reply)
    }
}
