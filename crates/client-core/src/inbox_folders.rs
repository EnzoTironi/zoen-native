//! Account-owned folders in the existing signed Personal log. No membership or relay grants.
use crate::{
    engine::{Engine, R},
    CoreError,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use types::{EventBody, InboxFolders, SpaceKind};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct InboxFoldersScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum InboxFoldersInput {
    Read {
        scope: InboxFoldersScope,
    },
    Put {
        scope: InboxFoldersScope,
        operation_id: String,
        observed_revision: String,
        configuration: InboxFolders,
    },
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct InboxFoldersResult {
    pub configuration: InboxFolders,
    pub revision: String,
    /// Exact receipt for a Put, including retries after later edits. Not a relay acknowledgement.
    pub applied_revision: Option<String>,
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: crate::i18n::t(
            "configuração de pastas inválida",
            "invalid folder configuration",
        ),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "as pastas mudaram; reveja a alteração",
            "folders changed; review the change",
        ),
    }
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_:".contains(&c))
}
fn identifiers(values: &[String]) -> bool {
    values.len() <= 200
        && values.iter().all(|s| identifier(s))
        && values.iter().collect::<HashSet<_>>().len() == values.len()
}
fn validate(configuration: &InboxFolders) -> R<()> {
    if configuration.folders.len() > 20 {
        return Err(invalid());
    }
    let mut ids = HashSet::new();
    for folder in &configuration.folders {
        if !folder.id.starts_with("folder:")
            || folder.id.len() <= 7
            || !identifier(&folder.id)
            || !ids.insert(&folder.id)
            || folder.name.trim() != folder.name
            || folder.name.is_empty()
            || folder.name.chars().count() > 32
            || folder.name.chars().any(char::is_control)
            || folder.kinds.len() > 3
            || !identifiers(&folder.include)
            || !identifiers(&folder.exclude)
            || folder
                .kinds
                .iter()
                .enumerate()
                .any(|(i, kind)| folder.kinds[..i].contains(kind))
        {
            return Err(invalid());
        }
    }
    if !["all", "unread", "Direct", "Group", "Community", "archived"]
        .contains(&configuration.default_folder.as_str())
        && !ids.contains(&configuration.default_folder)
    {
        return Err(invalid());
    }
    Ok(())
}
#[derive(Clone)]
struct Recorded {
    device: String,
    previous: String,
    configuration_hash: Vec<u8>,
    revision: String,
}
struct History {
    current: InboxFoldersResult,
    operations: HashMap<String, Recorded>,
}
impl History {
    fn read(engine: &Engine) -> R<Self> {
        let mut history = Self {
            current: InboxFoldersResult {
                configuration: InboxFolders::default(),
                revision: "0".into(),
                applied_revision: None,
            },
            operations: HashMap::new(),
        };
        let Some(personal) = engine.personal_space() else {
            return Ok(history);
        };
        if engine.broken_spaces.contains_key(&personal) || engine.net.synced.contains(&personal) {
            return Err(invalid());
        }
        let log = engine.logs.get(&personal).ok_or_else(invalid)?;
        let owner = engine.me.as_deref().ok_or_else(invalid)?;
        if log.events().first().is_none_or(|event| {
            event.author != owner
                || !matches!(
                    event.body,
                    EventBody::SpaceCreated {
                        kind: SpaceKind::Personal,
                        ..
                    }
                )
        }) {
            return Err(invalid());
        }
        for event in log.events() {
            let EventBody::InboxFoldersSet {
                configuration,
                operation_id,
                previous_revision,
            } = &event.body
            else {
                continue;
            };
            if event.author != owner {
                continue;
            }
            validate(configuration)?;
            if !identifier(operation_id)
                || *previous_revision != history.current.revision
                || history.operations.contains_key(operation_id)
            {
                return Err(invalid());
            }
            let revision = event_log::content_hash_of(event);
            history.operations.insert(
                operation_id.clone(),
                Recorded {
                    device: event.device.clone().ok_or_else(invalid)?,
                    previous: previous_revision.clone(),
                    configuration_hash: configuration_hash(configuration)?,
                    revision: revision.clone(),
                },
            );
            history.current.configuration = configuration.clone();
            history.current.revision = revision;
        }
        Ok(history)
    }
}
impl Engine {
    pub(crate) fn inbox_folders(&mut self, input: &InboxFoldersInput) -> R<InboxFoldersResult> {
        let scope = match input {
            InboxFoldersInput::Read { scope } | InboxFoldersInput::Put { scope, .. } => scope,
        };
        self.native_surface_owner(&scope.expected_identity_id, &scope.expected_device_id)?;
        self.check_private_message_view()?;
        let history = self.inbox_folder_history()?;
        let InboxFoldersInput::Put {
            operation_id,
            observed_revision,
            configuration,
            ..
        } = input
        else {
            return Ok(history.current.clone());
        };
        validate(configuration)?;
        if !identifier(operation_id) {
            return Err(invalid());
        }
        if let Some(record) = history.operations.get(operation_id) {
            if record.device != scope.expected_device_id
                || record.previous != *observed_revision
                || record.configuration_hash != configuration_hash(configuration)?
            {
                return Err(invalid());
            }
            let mut result = history.current.clone();
            result.applied_revision = Some(record.revision.clone());
            return Ok(result);
        }
        if *observed_revision != history.current.revision {
            return Err(stale());
        }
        let personal = self.ensure_personal_space()?;
        self.append(
            &personal,
            &scope.expected_identity_id,
            EventBody::InboxFoldersSet {
                configuration: configuration.clone(),
                operation_id: operation_id.clone(),
                previous_revision: observed_revision.clone(),
            },
        )?;
        let mut result = self.inbox_folder_history()?.current.clone();
        result.applied_revision = Some(result.revision.clone());
        Ok(result)
    }
}

fn configuration_hash(configuration: &InboxFolders) -> R<Vec<u8>> {
    Ok(Sha256::digest(serde_json::to_vec(configuration).map_err(|_| invalid())?).to_vec())
}
/// Disposable projection: the signed owner, Personal space and log head always qualify the cache.
pub(crate) struct FolderCache {
    key: (String, Option<String>, Option<String>),
    history: History,
}
impl Engine {
    fn inbox_folder_history(&mut self) -> R<&History> {
        let owner = self.me.clone().ok_or_else(invalid)?;
        let personal = self.personal_space();
        if personal
            .as_ref()
            .is_some_and(|id| self.broken_spaces.contains_key(id) || self.net.synced.contains(id))
        {
            return Err(invalid());
        }
        let head = personal
            .as_ref()
            .and_then(|id| self.logs.get(id))
            .and_then(|log| log.events().last())
            .map(event_log::content_hash_of);
        let key = (owner, personal, head);
        if self
            .inbox_folder_cache
            .as_ref()
            .is_none_or(|cache| cache.key != key)
        {
            self.inbox_folder_cache = Some(FolderCache {
                key,
                history: History::read(self)?,
            });
        }
        self.inbox_folder_cache
            .as_ref()
            .map(|cache| &cache.history)
            .ok_or_else(invalid)
    }
}
