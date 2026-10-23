//! Operational witnesses in the existing journal, never an execution permission.
use super::*;

impl JournalState {
    pub(in crate::agent) fn register_hold(
        path: &Path,
        request: &str,
        hash: &str,
        source: &str,
    ) -> Result<(), String> {
        if request.is_empty()
            || request.len() > 128
            || !Self::valid_hash(hash)
            || source.is_empty()
            || source.len() > 128
        {
            return Err("The held request binding is invalid".into());
        }
        Self::update(path, |state| {
            if let Some(old) = state.continuations.get(request) {
                if old.content_hash != hash || old.source.as_deref() != Some(source) {
                    return Err("The held request binding changed".into());
                }
                // Never manufacture a never-started witness for a legacy entry.
                return Ok(false);
            }
            state.continuations.insert(
                request.into(),
                ResumeAttempts {
                    content_hash: hash.into(),
                    attempts: 0,
                    source: Some(source.into()),
                    prepared: None,
                    progress: None,
                    registered: true,
                    outcome_unknown: false,
                },
            );
            Ok(true)
        })
        .map(|_| ())
    }

    pub(in crate::agent) fn renewal_safe(
        path: &Path,
        request: &str,
        hash: &str,
        source: &str,
    ) -> Result<bool, String> {
        match std::fs::symlink_metadata(path.with_extension("tmp")) {
            Ok(_) => return Err("The held journal needs reconciliation".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("The held journal could not be checked".into()),
        }
        let state = Self::load(path)?;
        Ok(state.continuations.get(request).is_some_and(|entry| {
            entry.content_hash == hash
                && entry.source.as_deref() == Some(source)
                && entry.registered
                && !entry.outcome_unknown
                && entry.prepared.is_none()
                && entry.progress.is_none()
                && entry.attempts < MAX_TRIES
        }))
    }

    pub(in crate::agent) fn entering_executor(
        path: &Path,
        request: &str,
        hash: &str,
        source: &str,
    ) -> Result<(), String> {
        Self::update(path, |state| {
            let entry = state
                .continuations
                .get_mut(request)
                .ok_or("The continuation attempt was not reserved")?;
            if entry.content_hash != hash
                || entry.source.as_deref() != Some(source)
                || entry.attempts == 0
            {
                return Err("The execution witness binding changed".into());
            }
            if entry.outcome_unknown {
                return Ok(false);
            }
            entry.outcome_unknown = true;
            Ok(true)
        })
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests;
