//! Attachment picker's views over the existing signed conversation projection.
use crate::{
    engine::{Engine, Entry, EntryBody, R},
    i18n::t,
    Client, CoreError, MediaDraftScope,
};
use serde::{Deserialize, Serialize};
use types::{AttachmentPresentation, FileDoc, ItemContent, ItemKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RecentAttachmentKind {
    File,
    Audio,
}
impl RecentAttachmentKind {
    pub(crate) fn index_kind(self) -> &'static str {
        match self {
            Self::File => "attachment_file",
            Self::Audio => "attachment_music",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RecentAttachmentReference {
    pub space_id: String,
    pub message_id: String,
    pub item_id: String,
    pub version: u32,
    pub sha256: String,
}
impl RecentAttachmentReference {
    pub(crate) fn is_valid(&self) -> bool {
        [&self.space_id, &self.message_id, &self.item_id]
            .iter()
            .all(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
            && self.version > 0
            && self.sha256.len() == 64
            && self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct RecentAttachment {
    pub reference: RecentAttachmentReference,
    pub name: String,
    pub mime: String,
    pub bytes: u64,
    pub at_ms: i64,
    pub space_title: String,
    pub author_title: String,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct RecentAttachmentPage {
    pub items: Vec<RecentAttachment>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RecentAttachmentSourceBinding {
    pub asset_id: String,
    pub kind: RecentAttachmentKind,
    pub reference: RecentAttachmentReference,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    scope: MediaDraftScope,
    kind: RecentAttachmentKind,
    query: String,
    generation: u64,
    at_ms: i64,
    space: String,
    message: String,
}

fn stale() -> CoreError {
    CoreError::Stale {
        reason: t(
            "A lista de anexos mudou. Abra novamente.",
            "The attachment list changed. Open it again.",
        ),
    }
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: t(
            "Referência ou consulta de anexo inválida.",
            "Invalid attachment reference or query.",
        ),
    }
}

impl Engine {
    /// Telegram's .file/.music tags have richer media attributes. This adaptation
    /// uses Zoen's actual MIME and signed presentation; it never changes those bytes.
    pub(crate) fn recent_attachment_classification<'a>(
        &'a self,
        entry: &Entry,
    ) -> Option<(RecentAttachmentKind, &'a FileDoc)> {
        let EntryBody::Message {
            attaches: Some(id),
            marks,
            ..
        } = &entry.body
        else {
            return None;
        };
        let me = self.me.as_deref()?;
        if entry.seq == 0
            || marks.deleted
            || self.state.is_recovery_item(id)
            || !self.is_member(&entry.space, me)
        {
            return None;
        }
        let item = self.state.items.get(id)?;
        if item.kind != ItemKind::File || item.space != entry.space || item.has_voice_history {
            return None;
        }
        let ItemContent::File(file) = &item.current().content else {
            return None;
        };
        crate::files::validate_file(file, 64 << 20).ok()?;
        if file.mime.starts_with("audio/") {
            return Some((RecentAttachmentKind::Audio, file));
        }
        if entry.author == me
            && (marks
                .media
                .as_ref()
                .is_some_and(|media| media.presentation == AttachmentPresentation::File)
                || (!file.mime.starts_with("image/") && !file.mime.starts_with("video/")))
        {
            return Some((RecentAttachmentKind::File, file));
        }
        None
    }

    fn ensure_recent_attachments(&mut self) -> R<()> {
        if self.approval_replay_needed {
            self.reproject_approval_overlay()?;
        }
        if self.index_dirty || !self.store.recent_attachments_ready()? {
            self.reindex()?;
        }
        Ok(())
    }

    fn recent_row(
        &self,
        kind: RecentAttachmentKind,
        row: &store::RecentAttachmentIndexRow,
    ) -> R<RecentAttachment> {
        let space = self.state.spaces.get(&row.space).ok_or_else(stale)?;
        let entry = usize::try_from(row.entry_index)
            .ok()
            .and_then(|index| space.entries.get(index))
            .ok_or_else(stale)?;
        let (actual_kind, file) = self
            .recent_attachment_classification(entry)
            .ok_or_else(stale)?;
        if actual_kind != kind
            || entry.id() != row.message
            || entry.author != row.author
            || entry.at_ms != row.at_ms
        {
            return Err(stale());
        }
        let EntryBody::Message {
            attaches: Some(item_id),
            ..
        } = &entry.body
        else {
            return Err(stale());
        };
        let version = self.state.items[item_id].current().number;
        self.file_demand_key(item_id, version, 64 << 20)?;
        Ok(RecentAttachment {
            reference: RecentAttachmentReference {
                space_id: row.space.clone(),
                message_id: row.message.clone(),
                item_id: item_id.clone(),
                version,
                sha256: file.sha256.clone(),
            },
            name: file.name.clone(),
            mime: file.mime.clone(),
            bytes: file.bytes,
            at_ms: entry.at_ms,
            space_title: space.title.clone(),
            author_title: self
                .identities
                .get(&entry.author)
                .map(|identity| identity.name.clone())
                .unwrap_or_default(),
            ready: self.file_dto(file).ready,
        })
    }

    pub(crate) fn recent_attachments(
        &mut self,
        scope: &MediaDraftScope,
        kind: RecentAttachmentKind,
        query: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> R<RecentAttachmentPage> {
        self.validate_private_draft_scope(&scope.normal())?;
        if !(1..=50).contains(&limit) || query.len() > 512 || query.contains('\0') {
            return Err(invalid());
        }
        self.ensure_recent_attachments()?;
        let generation = self.store.search_generation()?;
        let position: Option<Cursor> = cursor
            .map(|value| {
                if value.len() > 4096 {
                    return Err(invalid());
                }
                let cursor: Cursor = serde_json::from_str(value).map_err(|_| invalid())?;
                if cursor.scope != *scope
                    || cursor.kind != kind
                    || cursor.query != query
                    || cursor.generation != generation
                {
                    return Err(stale());
                }
                Ok(cursor)
            })
            .transpose()?;
        let rows = self.store.recent_attachments_page(
            kind.index_kind(),
            (kind == RecentAttachmentKind::File).then_some(scope.expected_identity_id.as_str()),
            query,
            position
                .as_ref()
                .map(|cursor| (cursor.at_ms, cursor.space.as_str(), cursor.message.as_str())),
            limit,
        );
        let mut rows = match rows {
            Ok(rows) => rows,
            Err(error) => {
                self.index_dirty = true;
                self.reindex()?;
                return Err(error.into());
            }
        };
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let items: R<Vec<_>> = rows.iter().map(|row| self.recent_row(kind, row)).collect();
        let items = match items {
            Ok(items) => items,
            Err(error) => {
                self.index_dirty = true;
                self.reindex()?;
                return Err(error);
            }
        };
        let next_cursor = if has_more {
            let row = rows.last().ok_or_else(invalid)?;
            Some(
                serde_json::to_string(&Cursor {
                    scope: scope.clone(),
                    kind,
                    query: query.into(),
                    generation,
                    at_ms: row.at_ms,
                    space: row.space.clone(),
                    message: row.message.clone(),
                })
                .map_err(|_| invalid())?,
            )
        } else {
            None
        };
        Ok(RecentAttachmentPage { items, next_cursor })
    }

    pub(crate) fn attachment_reference_validate(
        &mut self,
        scope: &MediaDraftScope,
        kind: RecentAttachmentKind,
        reference: &RecentAttachmentReference,
    ) -> R<RecentAttachment> {
        self.validate_private_draft_scope(&scope.normal())?;
        if !reference.is_valid() {
            return Err(invalid());
        }
        self.ensure_recent_attachments()?;
        let row = self
            .store
            .recent_attachment_reference(
                kind.index_kind(),
                &reference.space_id,
                &reference.message_id,
            )?
            .ok_or_else(stale)?;
        let current = self.recent_row(kind, &row)?;
        if current.reference != *reference {
            return Err(stale());
        }
        Ok(current)
    }
}

#[uniffi::export]
impl Client {
    pub fn recent_attachments(
        &self,
        scope: MediaDraftScope,
        kind: RecentAttachmentKind,
        query: String,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<RecentAttachmentPage, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock()
            .recent_attachments(&scope, kind, &query, cursor.as_deref(), limit)
    }
    pub fn attachment_reference_validate(
        &self,
        scope: MediaDraftScope,
        kind: RecentAttachmentKind,
        reference: RecentAttachmentReference,
    ) -> Result<RecentAttachment, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock()
            .attachment_reference_validate(&scope, kind, &reference)
    }
}

#[cfg(test)]
mod tests;
