//! Read markers remain private prefs; real read watermarks only move forward.
use super::{history::History, invalid, stale, ChatLayoutField, ChatLayoutInput, ChatLayoutResult};
use crate::{
    engine::{Engine, R},
    Client, CoreError,
};

pub(super) fn through(raw: Option<&str>) -> R<Option<u64>> {
    raw.map(|value| {
        let sequence = value.parse::<u64>().map_err(|_| invalid())?;
        if sequence == 0 || sequence.to_string() != value {
            return Err(invalid());
        }
        Ok(sequence)
    })
    .transpose()
}
fn corrupt_read() -> CoreError {
    CoreError::Storage {
        message: crate::i18n::t(
            "não foi possível verificar a leitura da conversa",
            "could not verify this chat's read state",
        ),
    }
}
impl Engine {
    pub(crate) fn read_target_seq(&self, space: &str) -> Option<u64> {
        self.state
            .spaces
            .get(space)?
            .entries
            .iter()
            .map(|entry| entry.seq)
            .filter(|seq| *seq > 0)
            .max()
    }
    fn checked_read_prefix(&self, space: &str) -> R<Option<u64>> {
        self.store
            .meta(&format!("read:{space}"))?
            .map(|raw| {
                let seq = raw.parse::<u64>().map_err(|_| corrupt_read())?;
                if seq.to_string() != raw {
                    return Err(corrupt_read());
                }
                Ok(seq)
            })
            .transpose()
    }
    /// Caller holds the existing Engine lock. The savepoint covers both existing authorities.
    pub(crate) fn commit_chat_read(
        &mut self,
        space: &str,
        target: Option<u64>,
        expected_manual_revision: Option<&str>,
    ) -> R<Option<u64>> {
        self.space_state(space)?;
        if let Some(account) = &self.net.account {
            self.chat_mute_owner(space, &account.identity, &account.device)?;
        }
        if target.is_some_and(|seq| self.read_target_seq(space).is_none_or(|max| seq > max)) {
            return Err(invalid());
        }
        let previous = self.checked_read_prefix(space)?;
        let history = History::read(self)?;
        let current = history.current(space);
        if expected_manual_revision
            .is_some_and(|revision| current.revision(ChatLayoutField::ManualUnread) != revision)
        {
            // An earlier read may already satisfy this exact prefix. This is a current
            // postcondition, not a claim about which request's acknowledgement was lost.
            if !current.prefs.manual_unread
                && target.is_none_or(|seq| previous.is_some_and(|read| read >= seq))
            {
                return Ok(None);
            }
            return Err(stale());
        }
        if current.prefs.manual_unread
            && self
                .personal_space()
                .is_some_and(|personal| self.net.synced.contains(&personal))
        {
            return Err(invalid());
        }
        let raise = target.filter(|seq| previous.is_none_or(|old| *seq > old));
        if !current.prefs.manual_unread && raise.is_none() {
            return Ok(None);
        }
        self.store.raw_execute("SAVEPOINT chat_read")?;
        let committed = (|| -> R<Option<u64>> {
            if current.prefs.manual_unread {
                let mut prefs = current.prefs;
                prefs.manual_unread = false;
                self.set_chat_prefs(space, prefs)?;
            }
            if let Some(seq) = raise {
                self.store
                    .set_meta(&format!("read:{space}"), &seq.to_string())?;
            }
            self.store.raw_execute("RELEASE chat_read")?;
            Ok(raise)
        })();
        if let Err(error) = committed {
            let rollback = self
                .store
                .raw_execute("ROLLBACK TO chat_read")
                .and_then(|_| self.store.raw_execute("RELEASE chat_read"));
            let reload = self.reload();
            rollback?;
            reload?;
            return Err(error);
        }
        committed
    }
}
impl Client {
    pub(crate) fn chat_layout_request(&self, input: &ChatLayoutInput) -> R<ChatLayoutResult> {
        let (result, signal) = {
            let mut engine = self.lock();
            let result = engine.chat_layout(input)?;
            let signal = match input {
                ChatLayoutInput::MarkRead {
                    scope, through_seq, ..
                } if engine.net.synced.contains(&scope.space_id) => {
                    through(through_seq.as_deref())?.map(|seq| (scope.space_id.clone(), seq))
                }
                _ => None,
            };
            (result, signal)
        };
        // The exact observed prefix is idempotent on the existing receipt path.
        // Manual unread itself never sends a receipt or alters anyone else's marks.
        if let Some((space, seq)) = signal {
            self.with_net(|net| net.ephemeral(space, protocol::EphemeralKind::Read { seq }));
        }
        Ok(result)
    }
}
