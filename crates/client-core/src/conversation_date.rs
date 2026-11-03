//! Read-only calendar navigation over the existing authorized timeline.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
use crate::{engine::Engine, message_drafts::DraftScope, CoreError, EntryKind, TimelineEntry};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(crate) struct MessageDateInput {
    pub scope: DraftScope,
    /// Half-open UTC interval for one calendar day in the reader's timezone.
    pub start_ms: String,
    pub end_ms: String,
}

fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: crate::i18n::t("data inválida", "invalid date"),
    }
}

fn interval(input: &MessageDateInput) -> Result<(i64, i64), CoreError> {
    let parse = |value: &str| {
        if value.len() > 16 {
            return Err(invalid());
        }
        let parsed = value.parse::<i64>().map_err(|_| invalid())?;
        if !(0..=8_640_000_000_000_000).contains(&parsed) || parsed.to_string() != value {
            return Err(invalid());
        }
        Ok(parsed)
    };
    let (start, end) = (parse(&input.start_ms)?, parse(&input.end_ms)?);
    // DST may make a local calendar day 23 or 25 hours; never assume 86,400,000ms.
    if !end
        .checked_sub(start)
        .is_some_and(|span| (1..=93_600_000).contains(&span))
    {
        return Err(invalid());
    }
    Ok((start, end))
}

impl Engine {
    pub(crate) fn message_date_target(
        &self,
        input: &MessageDateInput,
    ) -> Result<Option<String>, CoreError> {
        self.check_private_message_view()?;
        self.validate_private_draft_scope(&input.scope)?;
        if input.scope.thread_root_id.is_some() {
            return Err(invalid());
        }
        let (start, end) = interval(input)?;
        Ok(select(self.timeline(&input.scope.space_id)?, start, end))
    }
}

pub(crate) fn select(entries: Vec<TimelineEntry>, start: i64, end: i64) -> Option<String> {
    let mut best: Option<(i64, String)> = None;
    for entry in entries {
        if entry.deleted
            || entry.in_thread.is_some()
            || !matches!(entry.kind, EntryKind::Message { .. })
        {
            continue;
        }
        let at = entry.at_ms;
        if (start..end).contains(&at) && best.as_ref().is_none_or(|(old, _)| at < *old) {
            best = Some((at, entry.id));
        }
    }
    best.map(|(_, id)| id)
}
