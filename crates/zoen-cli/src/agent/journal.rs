//! Existing private continuation journal encoding, recovery and retention.

use super::*;

impl JournalState {
    pub(super) fn valid_hash(hash: &str) -> bool {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }
    pub(super) fn load(path: &Path) -> Result<Self, String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err("The tool journal could not be read".into()),
        };
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "The tool journal is corrupt")?;
        if value.is_array() {
            return Ok(Self {
                steps: serde_json::from_value(value).map_err(|_| "The tool journal is corrupt")?,
                continuations: BTreeMap::new(),
            });
        }
        let root = value.as_object().ok_or("The tool journal is corrupt")?;
        if root.len() != 3 || root.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
            return Err("The tool journal version is unsupported".into());
        }
        let steps = serde_json::from_value(
            root.get("steps")
                .cloned()
                .ok_or("The tool journal is corrupt")?,
        )
        .map_err(|_| "The tool journal is corrupt")?;
        let metadata = root
            .get("continuations")
            .and_then(serde_json::Value::as_object)
            .ok_or("The tool journal is corrupt")?;
        let mut continuations = BTreeMap::new();
        for (request, budget) in metadata {
            let budget = budget
                .as_object()
                .filter(|budget| {
                    budget.keys().all(|key| {
                        matches!(
                            key.as_str(),
                            "content_hash"
                                | "attempts"
                                | "source"
                                | "prepared"
                                | "progress"
                                | "registered"
                                | "outcome_unknown"
                        )
                    })
                })
                .ok_or("The continuation budget is corrupt")?;
            let content_hash = budget
                .get("content_hash")
                .and_then(serde_json::Value::as_str)
                .filter(|hash| Self::valid_hash(hash))
                .ok_or("The continuation budget is corrupt")?;
            let flag = |name: &str| -> Result<bool, String> {
                match budget.get(name) {
                    None => Ok(false),
                    Some(serde_json::Value::Bool(value)) => Ok(*value),
                    _ => Err("The continuation witness is corrupt".into()),
                }
            };
            let registered = flag("registered")?;
            let outcome_unknown = flag("outcome_unknown")?;
            let attempts = budget
                .get("attempts")
                .and_then(serde_json::Value::as_u64)
                .filter(|attempts| (*attempts > 0 || registered) && *attempts <= MAX_TRIES as u64)
                .ok_or("The continuation budget is corrupt")? as u32;
            if request.is_empty() || request.len() > 128 {
                return Err("The continuation request is invalid".into());
            }
            let source = match budget.get("source") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(source))
                    if !source.is_empty() && source.len() <= 128 =>
                {
                    Some(source.clone())
                }
                _ => return Err("The continuation source is corrupt".into()),
            };
            let prepared: Option<ToolAsk> = budget
                .get("prepared")
                .filter(|value| !value.is_null())
                .map(|value| {
                    serde_json::from_value(value.clone())
                        .map_err(|_| "The prepared tool is corrupt")
                })
                .transpose()?;
            let progress: Option<ToolProgress> = budget
                .get("progress")
                .filter(|value| !value.is_null())
                .map(|value| {
                    serde_json::from_value(value.clone())
                        .map_err(|_| "The completed progress is corrupt")
                })
                .transpose()?;
            if prepared.as_ref().is_some_and(|ask| {
                ask.version != 1
                    || ask.round >= 4
                    || ask.steps.len() >= 16
                    || ask.remaining.len() > 3
                    || ask.steps.len() + 1 + ask.remaining.len() > 16
            }) || progress.as_ref().is_some_and(|progress| {
                progress.round >= 4
                    || progress.steps.is_empty()
                    || progress.steps.len() > 16
                    || progress.remaining.len() > 3
                    || progress.steps.len() + progress.remaining.len() > 16
            }) {
                return Err("The continuation progress budget is corrupt".into());
            }
            if prepared.as_ref().is_some_and(|ask| {
                serde_json::to_vec(ask).map_or(true, |bytes| {
                    bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
                })
            }) || progress.as_ref().is_some_and(|progress| {
                serde_json::to_vec(progress).map_or(true, |bytes| {
                    bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
                })
            }) {
                return Err("The continuation progress exceeds its capacity".into());
            }
            continuations.insert(
                request.clone(),
                ResumeAttempts {
                    content_hash: content_hash.into(),
                    attempts,
                    source,
                    prepared,
                    progress,
                    registered,
                    outcome_unknown,
                },
            );
        }
        Ok(Self {
            steps,
            continuations,
        })
    }
    pub(super) fn bytes(&self) -> Result<Vec<u8>, String> {
        if self.continuations.is_empty() {
            return serde_json::to_vec(&self.steps)
                .map_err(|_| "The tool journal could not be encoded".into());
        }
        let continuations: serde_json::Map<String, serde_json::Value> = self
            .continuations
            .iter()
            .map(|(request, budget)| {
                let mut value = json!({"content_hash":budget.content_hash,"attempts":budget.attempts,"source":budget.source,"prepared":budget.prepared,"progress":budget.progress});
                // Omit new metadata for historical entries: their encoded bytes
                // and conservative unknown-execution semantics remain unchanged.
                if budget.registered { value["registered"] = true.into(); }
                if budget.outcome_unknown { value["outcome_unknown"] = true.into(); }
                (request.clone(), value)
            })
            .collect();
        serde_json::to_vec(&json!({"version":1,"steps":self.steps,"continuations":continuations}))
            .map_err(|_| "The tool journal could not be encoded".into())
    }
    /// Reserve the temp path before reading the old state so simultaneous local writers
    /// cannot overwrite a newer retry count with a stale snapshot. A leftover temp file
    /// after interruption refuses another attempt until the host/operator reconciles it.
    pub(super) fn update(
        path: &Path,
        apply: impl FnOnce(&mut Self) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let temporary = path.with_extension("tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| "The tool journal is busy or unwritable")?;
        struct Temporary(Option<std::path::PathBuf>);
        impl Drop for Temporary {
            fn drop(&mut self) {
                if let Some(path) = &self.0 {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let mut temporary_guard = Temporary(Some(temporary.clone()));
        let mut state = Self::load(path)?;
        if !apply(&mut state)? {
            return Ok(false);
        }
        file.write_all(&state.bytes()?)
            .map_err(|_| "The tool journal could not be written")?;
        file.sync_all()
            .map_err(|_| "The tool journal could not be synchronized")?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|_| "The tool journal could not be committed")?;
        temporary_guard.0 = None;
        #[cfg(unix)]
        {
            let directory = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::File::open(directory)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| "The tool journal directory could not be synchronized")?;
        }
        Ok(true)
    }
    pub(super) fn begin_resume(
        path: &Path,
        request: &str,
        content_hash: &str,
        source: Option<&str>,
    ) -> Result<bool, String> {
        if request.is_empty()
            || request.len() > 128
            || !Self::valid_hash(content_hash)
            || source.is_some_and(|source| source.is_empty() || source.len() > 128)
        {
            return Err("The continuation request is invalid".into());
        }
        Self::update(path, |state| {
            if let Some(budget) = state.continuations.get_mut(request) {
                if budget.content_hash != content_hash
                    || (source.is_some()
                        && budget
                            .source
                            .as_deref()
                            .is_some_and(|old| Some(old) != source))
                {
                    return Err("The continuation request hash changed".into());
                }
                if budget.attempts >= MAX_TRIES {
                    return Ok(false);
                }
                budget.attempts += 1;
                if budget.source.is_none() {
                    budget.source = source.map(str::to_string);
                }
            } else {
                state.continuations.insert(
                    request.into(),
                    ResumeAttempts {
                        content_hash: content_hash.into(),
                        attempts: 1,
                        source: source.map(str::to_string),
                        prepared: None,
                        progress: None,
                        registered: false,
                        outcome_unknown: false,
                    },
                );
            }
            Ok(true)
        })
    }
}

mod renewal;
mod turn_budget;
