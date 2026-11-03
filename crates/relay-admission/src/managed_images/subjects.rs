//! Current subject commitments share the checkpoint's atomic rename. They are
//! hashes of native state/policy, not copies of documents, credentials or money.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::state::{digest, hash, State};
use super::{AnchorError, AnchorReader};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_SUBJECTS: usize = 512;
const MAX_UPDATES: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectUpdate {
    pub subject: String,
    pub previous: Option<String>,
    pub next: String,
}

pub(super) fn well_formed(updates: &[SubjectUpdate]) -> bool {
    let mut unique = BTreeSet::new();
    updates.len() <= MAX_UPDATES
        && updates.iter().all(|update| {
            digest(&update.subject)
                && digest(&update.next)
                && update.previous.as_ref().is_none_or(|value| digest(value))
                && update.previous.as_ref() != Some(&update.next)
                && unique.insert(&update.subject)
        })
}

pub(super) fn apply(
    subjects: &BTreeMap<String, String>,
    updates: &[SubjectUpdate],
) -> Option<BTreeMap<String, String>> {
    if !well_formed(updates) {
        return None;
    }
    let mut next = subjects.clone();
    for update in updates {
        if subjects.get(&update.subject) != update.previous.as_ref() {
            return None;
        }
        next.insert(update.subject.clone(), update.next.clone());
    }
    valid(&next).then_some(next)
}

pub(super) fn valid(subjects: &BTreeMap<String, String>) -> bool {
    subjects.len() <= MAX_SUBJECTS
        && subjects
            .iter()
            .all(|(subject, value)| digest(subject) && digest(value))
}

pub(super) fn commitment(subjects: &BTreeMap<String, String>) -> Option<String> {
    if subjects.is_empty() {
        None
    } else {
        hash(&("runtime-continuity/subjects/1", subjects))
    }
}

impl State {
    pub(super) fn subjects_valid(&self) -> bool {
        valid(&self.subjects)
            && (self.deployment.schema() == 3 || self.subjects.is_empty())
            && self
                .committed_effect
                .as_ref()
                .and_then(|effect| effect.subjects_after.as_ref())
                == commitment(&self.subjects).as_ref()
    }
}

impl AnchorReader {
    /// Inert hashes only, never admission or a replacement for current stores.
    ///
    /// # Errors
    /// Refuses invalid/private-file failures, another profile or pending work.
    pub fn subject_commitments(&self) -> Result<BTreeMap<String, String>, AnchorError> {
        let state = self.state()?;
        if state.deployment.schema() != 3 || state.prepared.is_some() {
            return Err(AnchorError::Unreconciled);
        }
        Ok(state.subjects)
    }

    /// Expected hashes of the exact retained pending transition. Recovery must
    /// compare actual authenticated store contents before committing it.
    ///
    /// # Errors
    /// Refuses corrupt history, a different pending transition or invalid updates.
    pub fn pending_subject_commitments(
        &self,
        expected: &super::Prepared,
    ) -> Result<BTreeMap<String, String>, AnchorError> {
        let state = self.state()?;
        if state.deployment.schema() != 3 || state.prepared.as_ref() != Some(expected) {
            return Err(AnchorError::Unreconciled);
        }
        apply(&state.subjects, &expected.subject_updates).ok_or(AnchorError::Invalid)
    }

    /// Observation only. A match does not certify native custody, authorize a
    /// mutation, or attest the caller's current SQL/FDB transaction.
    ///
    /// # Errors
    /// Refuses malformed digests, pending work or a mismatching current value.
    pub fn check_subject(&self, subject: &str, observed: Option<&str>) -> Result<(), AnchorError> {
        if !digest(subject) || observed.is_some_and(|value| !digest(value)) {
            return Err(AnchorError::Invalid);
        }
        let state = self.state()?;
        if state.deployment.schema() != 3
            || state.prepared.is_some()
            || state.subjects.get(subject).map(String::as_str) != observed
        {
            return Err(AnchorError::Unreconciled);
        }
        Ok(())
    }
}
