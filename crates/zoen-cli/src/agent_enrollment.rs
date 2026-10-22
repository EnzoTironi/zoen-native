//! Operator enrollment glue. The core owns consent, prepared keys and recovery.
use crate::file_vault::FileVault;
use client_core::{AgentEnrollmentHostProgress, AgentEnrollmentHostRequest, Client, SecretVault};
use std::collections::BTreeMap;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;
use zoen_agentd::{AgentKind, Definition, ModelTier};

const REQUEST: &str = "agent-enroll-request";
const WAIT: &str = "agent-enroll-wait";
const CANCEL: &str = "agent-enroll-cancel";
const MAX_WAIT_MS: u64 = 15 * 60 * 1000;
const FILE_LIMIT: u64 = 16 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Request {
        owner: String,
        relay: String,
        name: String,
        handle: String,
        definition: PathBuf,
    },
    Wait {
        timeout_ms: u64,
    },
    Cancel {
        digest: String,
    },
}

pub(crate) fn requires_home_lock(command: &str) -> bool {
    matches!(command, "agent-host" | REQUEST | WAIT | CANCEL)
}

fn error(message: &str) -> String {
    message.to_string()
}

impl Action {
    pub(crate) fn parse(
        command: &str,
        args: &[String],
        timeout_ms: u64,
        offline: bool,
    ) -> Result<Option<Self>, String> {
        let allowed: &[&str] = match command {
            REQUEST => &["--owner", "--relay", "--name", "--handle", "--definition"],
            WAIT => &[],
            CANCEL => &["--offer-digest"],
            _ => return Ok(None),
        };
        let mut flags = BTreeMap::new();
        for pair in args.chunks(2) {
            if pair.len() != 2
                || !allowed.contains(&pair[0].as_str())
                || pair[1].is_empty()
                || pair[1].starts_with("--")
                || flags.insert(pair[0].as_str(), pair[1].clone()).is_some()
            {
                return Err(error(
                    "Invalid enrollment arguments. Use explicit named flags once.",
                ));
            }
        }
        let mut take = |name| {
            flags
                .remove(name)
                .ok_or_else(|| error("Enrollment requires every trusted configuration flag."))
        };
        let action = match command {
            REQUEST => Self::Request {
                owner: take("--owner")?,
                relay: take("--relay")?,
                name: take("--name")?,
                handle: take("--handle")?,
                definition: PathBuf::from(take("--definition")?),
            },
            WAIT => {
                if offline || timeout_ms == 0 || timeout_ms > MAX_WAIT_MS {
                    return Err(error(
                        "Enrollment wait requires a connection and a timeout from 1 to 900000ms.",
                    ));
                }
                Self::Wait { timeout_ms }
            }
            CANCEL => {
                let digest = take("--offer-digest")?;
                if digest.len() != 64
                    || !digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(error(
                        "Cancellation requires the exact observed offer digest.",
                    ));
                }
                Self::Cancel { digest }
            }
            _ => unreachable!(),
        };
        Ok(Some(action))
    }

    /// Read errors are never an invitation to replace a legacy host root. The
    /// core still validates every pending schema, key binding and permission.
    pub(crate) fn preflight(&self, vault: &FileVault) -> Result<(), String> {
        if matches!(self, Self::Request { .. }) && !std::io::stdout().is_terminal() {
            return Err(error(
                "Show the pairing code only in a trusted interactive terminal.",
            ));
        }
        preflight_slots(vault)
    }

    pub(crate) async fn run(
        self,
        client: &Arc<Client>,
        vault: Arc<FileVault>,
    ) -> Result<String, String> {
        let secrets: Arc<dyn SecretVault> = vault;
        match self {
            Self::Request {
                owner,
                relay,
                name,
                handle,
                definition,
            } => {
                // Recheck the output boundary before creating a one-time code.
                if !std::io::stdout().is_terminal() {
                    return Err(error("Pairing requires a trusted interactive terminal."));
                }
                let definition_json = String::from_utf8(read_definition(&definition)?)
                    .map_err(|_| error("Invalid enrollment definition."))?;
                let offer = client
                    .prepare_agent_enrollment(
                        AgentEnrollmentHostRequest {
                            expected_owner_id: owner,
                            relay_url: relay,
                            name,
                            handle,
                            definition_json,
                        },
                        secrets,
                    )
                    .map_err(|_| {
                        error("Enrollment could not be prepared. Retained setup was preserved.")
                    })?;
                // Intentionally shown only to the operator's terminal. Never
                // return/debug the secret offer or include it in test reports.
                println!("code: {}\ncheck: {}", offer.code, offer.pairing_check);
                Ok(format!(
                    "offer={} expires_ms={}",
                    offer.offer_digest, offer.expires_ms
                ))
            }
            Self::Wait { timeout_ms } => {
                let progress = match client.resume_agent_enrollment(secrets.clone())
                    .map_err(|_| error("Enrollment recovery failed closed. Retained setup was preserved."))? {
                    Some(progress) => progress,
                    None => client.wait_agent_enrollment(secrets, timeout_ms).await
                        .map_err(|_| error("Enrollment was not confirmed. Resume the same retained setup; do not reset it."))?,
                };
                finish(client, progress, timeout_ms).await
            }
            Self::Cancel { digest } => {
                let cancelled = client.cancel_agent_enrollment(secrets, digest).await
                    .map_err(|_| error("Enrollment was not cancelled. Accepted or uncertain setup was preserved."))?;
                Ok(if cancelled {
                    "Unused offer cancelled; the host identity was retained."
                } else {
                    "No matching unused offer was cancelled."
                }
                .into())
            }
        }
    }
}

fn preflight_slots(vault: &FileVault) -> Result<(), String> {
    for (key, limit, seed) in [
        ("agent_root", 32, true),
        ("zoen.identity.v1", 32, true),
        ("zoen.device.v1", 32, true),
        ("zoen.agreement.v1", 32, true),
        ("agent.enrollment.pending.v1", FILE_LIMIT, false),
        ("zoen.provider-setup.v1", 0, false),
        ("link.pending", 0, false),
    ] {
        let metadata = match std::fs::symlink_metadata(vault.dir.join(key)) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                return Err(error(
                    "Existing protected setup could not be read. Nothing was replaced.",
                ));
            }
        };
        if !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() > limit
            || (seed && metadata.len() != 32)
        {
            return Err(error(
                "Existing protected setup is incompatible. Nothing was replaced.",
            ));
        }
        let bytes = Zeroizing::new(vault.read_bytes(key).map_err(|_| {
            error("Existing protected setup could not be read. Nothing was replaced.")
        })?);
        if (seed && bytes.len() != 32)
            || (!seed
                && (bytes.is_empty()
                    || bytes.len() as u64 > limit
                    || std::str::from_utf8(&bytes).is_err()))
        {
            return Err(error(
                "Existing protected setup is malformed. Nothing was replaced.",
            ));
        }
    }
    Ok(())
}

fn definition(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let invalid = || {
        error(
            "Use an explicit User v1 definition with Cheap tier and empty state; unsupported behavior is not substituted.",
        )
    };
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let object = value.as_object().ok_or_else(invalid)?;
    let fields = ["version", "kind", "persona", "rules", "state", "model_tier"];
    if object.len() != fields.len() || fields.iter().any(|key| !object.contains_key(*key)) {
        return Err(invalid());
    }
    // Struct decoding rejects duplicated known fields; the object inventory
    // above rejects unknown fields that Definition otherwise ignores.
    let parsed: Definition = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if parsed.version != 1
        || parsed.kind != AgentKind::User
        || parsed.model_tier != ModelTier::Cheap
        || !parsed
            .state
            .as_object()
            .is_some_and(|state| state.is_empty())
        || parsed.persona.is_empty()
        || parsed.persona.len() > 768
        || parsed.persona.chars().any(|c| c.is_control() && c != '\n')
        || parsed.rules.len() > 8
        || parsed
            .rules
            .iter()
            .any(|r| r.is_empty() || r.len() > 256 || r.chars().any(char::is_control))
    {
        return Err(invalid());
    }
    let encoded = parsed.encode().map_err(|_| invalid())?;
    if encoded.len() > 1024 {
        return Err(invalid());
    }
    Ok(encoded)
}

fn read_definition(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path)
        .map_err(|_| error("Could not read the enrollment definition file."))?;
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("Could not read the enrollment definition file."))?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(error("Enrollment definition file is too large."));
    }
    definition(&bytes)
}

#[derive(Debug, PartialEq, Eq)]
enum DefinitionState {
    Absent,
    Confirmed,
}

fn definition_state(
    status: &str,
    version: u32,
    kind: &str,
    bytes: &[u8],
    expected: &[u8],
) -> Result<DefinitionState, String> {
    if !matches!(status, "active" | "paused") {
        return Err(error(
            "The relay did not confirm an active or paused Agent.",
        ));
    }
    if version == 0 && kind.is_empty() && bytes.is_empty() {
        return Ok(DefinitionState::Absent);
    }
    if version == 1 && kind == "User" && bytes == expected {
        return Ok(DefinitionState::Confirmed);
    }
    Err(error(
        "The relay definition differs from the reviewed enrollment. Nothing was overwritten.",
    ))
}

struct SyncGuard(Arc<Client>);
impl Drop for SyncGuard {
    fn drop(&mut self) {
        self.0.stop_sync();
    }
}

fn account_matches(client: &Client, progress: &AgentEnrollmentHostProgress) -> bool {
    client.account().is_some_and(|account| {
        account.identity_id == progress.agent_id && account.relay_url == progress.relay_url
    }) && client.agent_owner().as_deref() == Some(progress.owner_id.as_str())
}

async fn finish(
    client: &Arc<Client>,
    progress: AgentEnrollmentHostProgress,
    timeout_ms: u64,
) -> Result<String, String> {
    let expected = definition(progress.definition_json.as_bytes())?;
    if expected != progress.definition_json.as_bytes() || !account_matches(client, &progress) {
        return Err(error(
            "The retained account or definition no longer matches this enrollment.",
        ));
    }
    let _sync = SyncGuard(client.clone());
    client
        .start_sync(None)
        .map_err(|_| error("Registration could not start. Retained setup was preserved."))?;
    let connection = client.wait_until_idle(timeout_ms).await;
    if connection.state != "online"
        || !client.account().is_some_and(|a| a.registered)
        || !account_matches(client, &progress)
    {
        return Err(error(
            "Registration was not confirmed by the relay. Resume the same retained setup.",
        ));
    }
    let budget = Duration::from_millis(timeout_ms);
    let current = tokio::time::timeout(budget, client.agent_state())
        .await
        .map_err(|_| error("Relay state is uncertain. Retained setup was preserved."))?
        .map_err(|_| {
            error("Relay state was refused or unavailable. Retained setup was preserved.")
        })?;
    if definition_state(
        &current.status,
        current.definition_version,
        &current.kind,
        &current.definition,
        &expected,
    )? == DefinitionState::Absent
    {
        tokio::time::timeout(
            budget,
            client.agent_define(
                progress.agent_id.clone(),
                1,
                "User".into(),
                expected.clone(),
            ),
        )
        .await
        .map_err(|_| {
            error("Definition publication is uncertain. Resume to reconcile; do not reset.")
        })?
        .map_err(|_| {
            error("Definition publication was not confirmed. Resume to reconcile; do not reset.")
        })?;
    }
    let confirmed = tokio::time::timeout(budget, client.agent_state())
        .await
        .map_err(|_| error("Definition confirmation is uncertain. Retained setup was preserved."))?
        .map_err(|_| {
            error("Definition confirmation was unavailable. Retained setup was preserved.")
        })?;
    if definition_state(
        &confirmed.status,
        confirmed.definition_version,
        &confirmed.kind,
        &confirmed.definition,
        &expected,
    )? != DefinitionState::Confirmed
        || !account_matches(client, &progress)
    {
        return Err(error(
            "The relay did not confirm the exact reviewed definition. Retained setup was preserved.",
        ));
    }
    Ok(format!(
        "registered agent={} definition=v1 status={}; runtime not started",
        progress.agent_id, confirmed.status
    ))
}

#[cfg(test)]
mod tests;
