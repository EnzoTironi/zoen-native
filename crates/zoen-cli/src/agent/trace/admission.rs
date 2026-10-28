//! Opt-in canonical admission metadata, with no error text or authority.
use super::{correlate, correlate_request};
use client_core::CoreError;
use std::io::{self, Write};

const EVENT: &str = "zoen.agent.admission.v1";
const MAX_LINE: usize = 512;

#[derive(Clone, Copy, Debug)]
pub(in crate::agent) enum Phase {
    SelectionBefore,
    DeckPermit,
    PagePermit,
    ScenePermit,
    ConversationPermit,
    SelectionAfter,
    FamilySelection,
}
impl Phase {
    fn id(self) -> &'static str {
        match self {
            Self::SelectionBefore => "selection_before",
            Self::DeckPermit => "deck_permit",
            Self::PagePermit => "page_permit",
            Self::ScenePermit => "scene_permit",
            Self::ConversationPermit => "conversation_permit",
            Self::SelectionAfter => "selection_after",
            Self::FamilySelection => "family_selection",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Variant {
    Storage,
    NotFound,
    Forbidden,
    Invalid,
    Stale,
    DraftRejected,
    NotApplicable,
    Unknown,
}
impl Variant {
    fn id(self) -> &'static str {
        match self {
            Self::Storage => "storage",
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::Invalid => "invalid",
            Self::Stale => "stale",
            Self::DraftRejected => "draft_rejected",
            Self::NotApplicable => "not_applicable",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Code {
    CoreRefused,
    SelectionRefused,
    ExecutorUnavailable,
    UnsupportedSchema,
}
impl Code {
    fn id(self) -> &'static str {
        match self {
            Self::CoreRefused => "core_refused",
            Self::SelectionRefused => "selection_refused",
            Self::ExecutorUnavailable => "executor_unavailable",
            Self::UnsupportedSchema => "unsupported_schema",
        }
    }
}

/// Only closed tags survive classification. Neither Display nor any error field
/// is retained; a `stale` tag cannot distinguish expiry from every stale context.
#[derive(Clone, Copy, Debug)]
pub(in crate::agent) struct Refusal {
    phase: Phase,
    variant: Variant,
    code: Code,
}
impl Refusal {
    pub(in crate::agent) fn core(phase: Phase, error: CoreError) -> Self {
        let variant = match error {
            CoreError::Storage { .. } => Variant::Storage,
            CoreError::NotFound { .. } => Variant::NotFound,
            CoreError::Forbidden { .. } => Variant::Forbidden,
            CoreError::Invalid { .. } => Variant::Invalid,
            CoreError::Stale { .. } => Variant::Stale,
            CoreError::DraftRejected { .. } => Variant::DraftRejected,
        };
        Self {
            phase,
            variant,
            code: Code::CoreRefused,
        }
    }
    pub(in crate::agent) fn selection(phase: Phase) -> Self {
        Self {
            phase,
            variant: Variant::Unknown,
            code: Code::SelectionRefused,
        }
    }
    pub(in crate::agent) fn executor() -> Self {
        Self {
            phase: Phase::DeckPermit,
            variant: Variant::NotApplicable,
            code: Code::ExecutorUnavailable,
        }
    }
    pub(in crate::agent) fn family() -> Self {
        Self {
            phase: Phase::FamilySelection,
            variant: Variant::NotApplicable,
            code: Code::UnsupportedSchema,
        }
    }
}

fn enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

fn write_refusal(
    on: bool,
    space: &str,
    source: &str,
    request: &str,
    refusal: Refusal,
    writer: &mut impl Write,
) {
    if !on {
        return;
    }
    let value = std::collections::BTreeMap::from([
        ("event", EVENT.into()),
        ("correlation", correlate(space, source)),
        (
            "requestCorrelation",
            correlate_request(space, source, request),
        ),
        ("phase", refusal.phase.id().into()),
        ("variant", refusal.variant.id().into()),
        ("code", refusal.code.id().into()),
    ]);
    if let Ok(mut line) = serde_json::to_vec(&value) {
        line.push(b'\n');
        if line.len() <= MAX_LINE {
            // Diagnostic IO never changes admission, retries or task outcomes.
            let _ = writer.write_all(&line);
        }
    }
}

pub(in crate::agent) fn refused(space: &str, source: &str, request: &str, refusal: Refusal) {
    if enabled(
        std::env::var("ZOEN_AGENT_OBSERVE_ADMISSION")
            .ok()
            .as_deref(),
    ) {
        write_refusal(
            true,
            space,
            source,
            request,
            refusal,
            &mut io::stdout().lock(),
        );
    }
}

#[cfg(test)]
mod tests;
