use serde::{Deserialize, Serialize};

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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub schema: u8,
    pub deployment: Deployment,
    pub committed: Checkpoint,
    pub prepared: Option<Prepared>,
}

/// Recovery never recovers a Fresh provider permit. Even BothAfter only
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
    pub fn initial(deployment: Deployment) -> Option<Self> {
        if !deployment.valid() {
            return None;
        }
        let committed = Checkpoint {
            generation: 1,
            sequence: 0,
            digest: hash(&("managed-images/genesis/1", &deployment))?,
        };
        Some(Self {
            schema: 1,
            deployment,
            committed,
            prepared: None,
        })
    }

    pub fn valid(&self) -> bool {
        self.schema == 1
            && self.deployment.valid()
            && self.committed.generation > 0
            && self.committed.generation <= i64::MAX as u64
            && self.committed.sequence <= i64::MAX as u64
            && digest(&self.committed.digest)
            && self.prepared.as_ref().is_none_or(|prepared| {
                self.prepare(&prepared.effect_digest, prepared.retirement)
                    .as_ref()
                    == Some(prepared)
            })
    }

    pub fn prepare(&self, effect: &str, retirement: bool) -> Option<Prepared> {
        if !digest(effect) {
            return None;
        }
        let sequence = self.committed.sequence.checked_add(1)?;
        let generation = if retirement {
            self.committed.generation.checked_add(1)?
        } else {
            self.committed.generation
        };
        if sequence > i64::MAX as u64 || generation > i64::MAX as u64 {
            return None;
        }
        Some(Prepared {
            previous: self.committed.clone(),
            next: Checkpoint {
                generation,
                sequence,
                digest: hash(&(
                    "managed-images/effect/1",
                    &self.deployment,
                    &self.committed,
                    sequence,
                    effect,
                    retirement,
                ))?,
            },
            effect_digest: effect.into(),
            retirement,
        })
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

fn hash(value: &impl Serialize) -> Option<String> {
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
