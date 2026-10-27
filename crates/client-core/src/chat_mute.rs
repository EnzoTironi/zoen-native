//! Timed local chat preferences. The existing signed Personal log is the only authority;
//! a snapshot clock anchors relative choices, so lost-ACK retries cannot extend expiry.
use crate::{
    engine::{Engine, R},
    CoreError,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatMuteMode {
    For {
        duration_seconds: u32,
        from_ms: String,
    },
    Until {
        until_ms: String,
    },
    Forever,
    Off,
}

pub(crate) fn clock_ms() -> Option<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .filter(|n| *n > 0)
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: crate::i18n::t(
            "prazo de silenciamento inválido ou relógio indisponível",
            "invalid mute deadline or unavailable clock",
        ),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "o silenciamento mudou; reveja a conversa",
            "mute preference changed; review the conversation",
        ),
    }
}
fn decimal(s: &str) -> R<i64> {
    let n = s.parse::<i64>().map_err(|_| invalid())?;
    if n <= 0 || n.to_string() != s {
        return Err(invalid());
    }
    Ok(n)
}
fn target(mode: &ChatMuteMode) -> R<(Option<i64>, Option<i64>)> {
    match mode {
        ChatMuteMode::Off => Ok((None, None)),
        ChatMuteMode::Forever => Ok((Some(i64::MAX), None)),
        ChatMuteMode::Until { until_ms } => {
            let n = decimal(until_ms)?;
            if n == i64::MAX {
                return Err(invalid());
            }
            Ok((Some(n), None))
        }
        ChatMuteMode::For {
            duration_seconds,
            from_ms,
        } => {
            if *duration_seconds == 0 {
                return Err(invalid());
            }
            let from = decimal(from_ms)?;
            let end = from
                .checked_add(i64::from(*duration_seconds) * 1000)
                .filter(|n| *n < i64::MAX)
                .ok_or_else(invalid)?;
            Ok((Some(end), Some(from)))
        }
    }
}

impl Engine {
    pub(crate) fn chat_mute_owner(&self, space: &str, identity: &str, device: &str) -> R<()> {
        self.native_surface_owner(identity, device)?;
        self.space_state(space)?;
        if !self.is_member(space, identity) {
            return Err(CoreError::Forbidden {
                reason: crate::i18n::t("o acesso à conversa mudou", "conversation access changed"),
            });
        }
        Ok(())
    }
    pub(crate) fn set_chat_mute(
        &mut self,
        space: &str,
        mode: &ChatMuteMode,
        expected: Option<&str>,
        identity: &str,
        device: &str,
    ) -> R<()> {
        self.set_chat_mute_at(space, mode, expected, identity, device, clock_ms())
    }
    // One supplied Core clock sample; production always uses clock_ms above. Tests
    // exercise exact expiry and delayed retry without a global/production clock override.
    pub(crate) fn set_chat_mute_at(
        &mut self,
        space: &str,
        mode: &ChatMuteMode,
        expected: Option<&str>,
        identity: &str,
        device: &str,
        now: Option<i64>,
    ) -> R<()> {
        self.chat_mute_owner(space, identity, device)?;
        let expected = expected.map(decimal).transpose()?;
        let (until, anchor) = target(mode)?;
        let mut prefs = self
            .state
            .chat_prefs
            .get(space)
            .cloned()
            .unwrap_or_default();
        // An exact canonical target proves the requested preference, including an
        // ACK arriving after expiry. It never starts a new relative duration.
        if prefs.muted_until_ms == until {
            return Ok(());
        }
        let active = prefs.muted_until_ms.filter(|n| *n > now.unwrap_or(0));
        if active != expected {
            return Err(stale());
        }
        if let Some(until) = until.filter(|n| *n != i64::MAX) {
            let now = now.ok_or_else(invalid)?;
            if anchor.is_some_and(|from| from > now) {
                return Err(invalid());
            }
            if until <= now {
                return Err(stale());
            }
        }
        prefs.muted_until_ms = until;
        self.set_chat_prefs(space, prefs)
    }
    /// Layout-only callers retain the latest raw mute deadline under the same lock.
    pub(crate) fn set_chat_layout_prefs(
        &mut self,
        space: &str,
        pinned: bool,
        archived: bool,
    ) -> R<()> {
        let mut prefs = self
            .state
            .chat_prefs
            .get(space)
            .cloned()
            .unwrap_or_default();
        prefs.pinned = pinned;
        prefs.archived = archived;
        self.set_chat_prefs(space, prefs)
    }
}
