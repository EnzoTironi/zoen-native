use super::deployment::Binding;
use super::{subjects, SubjectUpdate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Locators bound to an actual SQL/FDB pairing and certified managed device.
/// This structure is an observation, never a permission to dispatch or publish.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployment {
    pub namespace: String,
    pub witness: String,
    pub source: String,
    pub agent: String,
    pub device: String,
}
impl std::fmt::Debug for Deployment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Deployment { private pairing redacted }")
    }
}

impl Deployment {
    pub(super) fn valid(&self) -> bool {
        [&self.namespace, &self.source, &self.agent, &self.device]
            .into_iter()
            .all(|value| {
                !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
            })
            && digest(&self.witness)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub generation: u64,
    pub sequence: u64,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepared {
    pub previous: Checkpoint,
    pub next: Checkpoint,
    pub effect_digest: String,
    pub retirement: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_updates: Vec<SubjectUpdate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subjects_after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub schema: u8,
    pub deployment: Binding,
    pub committed: Checkpoint,
    pub committed_effect: Option<Prepared>,
    pub prepared: Option<Prepared>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub subjects: BTreeMap<String, String>,
}

/// Recovery never recovers a `Fresh` provider permit. Even `BothAfter` only
/// acknowledges durable evidence; a caller must inspect its original attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    Clean,
    BothBefore,
    BothAfter,
    SqlBefore,
    FdbBefore,
}

impl State {
    pub fn initial(deployment: Binding) -> Option<Self> {
        if !deployment.valid() {
            return None;
        }
        let committed = Checkpoint {
            generation: 1,
            sequence: 0,
            digest: hash(&(deployment.genesis_domain(), &deployment))?,
        };
        Some(Self {
            schema: deployment.schema(),
            deployment,
            committed,
            committed_effect: None,
            prepared: None,
            subjects: BTreeMap::new(),
        })
    }

    pub fn valid(&self) -> bool {
        self.schema == self.deployment.schema()
            && self.deployment.valid()
            && self.subjects_valid()
            && self.committed.generation > 0
            && i64::try_from(self.committed.generation).is_ok()
            && i64::try_from(self.committed.sequence).is_ok()
            && digest(&self.committed.digest)
            && match &self.committed_effect {
                None => {
                    self.committed.sequence == 0
                        && self.committed.generation == 1
                        && hash(&(self.deployment.genesis_domain(), &self.deployment)).as_ref()
                            == Some(&self.committed.digest)
                }
                Some(last) => {
                    last.next == self.committed
                        && transition(
                            &self.deployment,
                            &last.previous,
                            &last.effect_digest,
                            last.retirement,
                            &last.subject_updates,
                            last.subjects_after.as_deref(),
                            last.intent.as_deref(),
                        )
                        .as_ref()
                            == Some(last)
                }
            }
            && self.prepared.as_ref().is_none_or(|prepared| {
                self.prepare_subjects(
                    &prepared.effect_digest,
                    prepared.retirement,
                    &prepared.subject_updates,
                    prepared.intent.as_deref(),
                )
                .as_ref()
                    == Some(prepared)
            })
    }

    pub fn prepare_subjects(
        &self,
        effect: &str,
        retirement: bool,
        updates: &[SubjectUpdate],
        intent: Option<&str>,
    ) -> Option<Prepared> {
        let next = subjects::apply(&self.subjects, updates)?;
        transition(
            &self.deployment,
            &self.committed,
            effect,
            retirement,
            updates,
            subjects::commitment(&next).as_deref(),
            intent,
        )
    }

    pub fn recovery(&self, sql: &Checkpoint, fdb: &Checkpoint) -> Option<Recovery> {
        match &self.prepared {
            None if sql == &self.committed && fdb == &self.committed => Some(Recovery::Clean),
            Some(p) if sql == &p.previous && fdb == &p.previous => Some(Recovery::BothBefore),
            Some(p) if sql == &p.next && fdb == &p.next => Some(Recovery::BothAfter),
            Some(p) if sql == &p.previous && fdb == &p.next => Some(Recovery::SqlBefore),
            Some(p) if sql == &p.next && fdb == &p.previous => Some(Recovery::FdbBefore),
            _ => None,
        }
    }
}

pub(super) fn transition(
    deployment: &Binding,
    previous: &Checkpoint,
    effect: &str,
    retirement: bool,
    updates: &[SubjectUpdate],
    subjects_after: Option<&str>,
    intent: Option<&str>,
) -> Option<Prepared> {
    if !digest(effect)
        || !subjects::well_formed(updates)
        || !digest(&previous.digest)
        || previous.generation == 0
        || (deployment.schema() != 3
            && (!updates.is_empty() || subjects_after.is_some() || intent.is_some()))
        || subjects_after.is_some_and(|value| !digest(value))
        || (!updates.is_empty() && subjects_after.is_none())
        || intent.is_some_and(|value| {
            value.is_empty()
                || value.len() > 16_384
                || !value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'-' | b'_')
                })
        })
    {
        return None;
    }
    let sequence = previous.sequence.checked_add(1)?;
    let generation = if retirement {
        previous.generation.checked_add(1)?
    } else {
        previous.generation
    };
    if sequence > i64::MAX as u64 || generation > i64::MAX as u64 {
        return None;
    }
    Some(Prepared {
        previous: previous.clone(),
        next: Checkpoint {
            generation,
            sequence,
            digest: if subjects_after.is_some() || intent.is_some() {
                hash(&(
                    "runtime-continuity/subject-effect/1",
                    deployment,
                    previous,
                    sequence,
                    effect,
                    retirement,
                    updates,
                    subjects_after,
                    intent,
                ))?
            } else {
                hash(&(
                    deployment.effect_domain(),
                    deployment,
                    previous,
                    sequence,
                    effect,
                    retirement,
                ))?
            },
        },
        effect_digest: effect.into(),
        retirement,
        subject_updates: updates.into(),
        subjects_after: subjects_after.map(str::to_owned),
        intent: intent.map(str::to_owned),
    })
}

pub(super) fn hash(value: &impl Serialize) -> Option<String> {
    serde_json::to_vec(value)
        .ok()
        .map(|bytes| event_log::sha256_hex(&bytes))
}

pub(super) fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
