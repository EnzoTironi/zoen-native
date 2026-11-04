//! Device-private listening/viewing checkpoints, never messages or replicated read receipts.
use crate::{
    engine::{Engine, R},
    message_drafts::DraftScope,
    CoreError, EntryKind,
};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
const DOMAIN: &[u8] = b"zoen.media.playback.v1";
const MAX_BYTES: usize = 512 * 1024;
const MAX_ENTRIES: usize = 128;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MediaPlaybackScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub message_id: String,
    pub item_id: String,
    pub version: u32,
    pub sha256: String,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MediaPlaybackPosition {
    pub position_ms: u32,
    pub duration_ms: u32,
    pub rate_milli: u32,
}
impl MediaPlaybackPosition {
    fn valid(self) -> bool {
        self.duration_ms > 0
            && self.duration_ms <= 7 * 24 * 60 * 60 * 1000
            && self.position_ms <= self.duration_ms
            && matches!(self.rate_milli, 500 | 1000 | 1500 | 2000)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MediaPlaybackBookmark {
    pub revision: String,
    pub position: Option<MediaPlaybackPosition>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    scope: MediaPlaybackScope,
    bookmark: MediaPlaybackBookmark,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    entries: Vec<Saved>,
}
struct Storage {
    record: Record,
    id: String,
    aad: Vec<u8>,
    key: Zeroizing<[u8; 32]>,
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "The media playback checkpoint is invalid or changed.".into(),
    }
}
fn storage() -> CoreError {
    CoreError::Storage {
        message: "Could not recover media playback progress.".into(),
    }
}
fn id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
impl Engine {
    fn playback_record(&self, scope: &MediaPlaybackScope) -> R<Storage> {
        self.native_surface_owner(&scope.expected_identity_id, &scope.expected_device_id)?;
        self.check_private_message_view()?;
        self.validate_private_draft_scope(&DraftScope {
            expected_identity_id: scope.expected_identity_id.clone(),
            expected_device_id: scope.expected_device_id.clone(),
            space_id: scope.space_id.clone(),
            thread_root_id: None,
        })?;
        if !id(&scope.message_id) || !id(&scope.item_id) || scope.version == 0 {
            return Err(invalid());
        }
        let source = self
            .space_state(&scope.space_id)?
            .entries
            .iter()
            .find(|entry| entry.id() == scope.message_id && self.visible_message(entry))
            .ok_or_else(invalid)?;
        let entry = self.entry_dto(source);
        if entry.deleted {
            return Err(invalid());
        }
        let file = self.file_info_at(&scope.item_id, Some(scope.version))?;
        if file.sha256 != scope.sha256
            || !(file.mime.starts_with("audio/") || file.mime.starts_with("video/"))
        {
            return Err(invalid());
        }
        let pinned = match &entry.kind {
            EntryKind::Message {
                card: Some(card), ..
            } => card.item_id == scope.item_id && card.version == scope.version,
            _ => false,
        };
        let voice = entry.voice.as_ref().is_some_and(|voice| {
            voice.item_id == scope.item_id
                && voice.sha256 == scope.sha256
                && self
                    .state
                    .items
                    .get(&scope.item_id)
                    .is_some_and(|item| item.versions.len() == scope.version as usize)
        });
        if !pinned && !voice {
            return Err(invalid());
        }
        let aad = serde_json::to_vec(&(
            "zoen.media.playback.v1",
            &scope.expected_identity_id,
            &scope.expected_device_id,
        ))
        .map_err(|_| storage())?;
        let key_id = format!("media.playback.{}", hex::encode(Sha256::digest(&aad)));
        let author = self.author_for(&scope.expected_identity_id)?;
        let mut key = Zeroizing::new([0; 32]);
        Hkdf::<Sha256>::new(Some(DOMAIN), &author.key.secret())
            .expand(b"local playback encryption", &mut *key)
            .map_err(|_| storage())?;
        let record: Record = match self.store.meta(&key_id)? {
            Some(encoded) => {
                let plain = Zeroizing::new(
                    crate::local_sealed::open(&key, &aad, &encoded, MAX_BYTES)
                        .map_err(|()| storage())?,
                );
                serde_json::from_slice(&plain).map_err(|_| storage())?
            }
            None => Record::default(),
        };
        if record.entries.len() > MAX_ENTRIES
            || record.entries.iter().any(|entry| {
                !id(&entry.bookmark.revision)
                    || entry
                        .bookmark
                        .position
                        .is_some_and(|position| !position.valid())
            })
        {
            return Err(storage());
        }
        Ok(Storage {
            record,
            id: key_id,
            aad,
            key,
        })
    }
    pub(crate) fn media_playback_read(
        &self,
        scope: &MediaPlaybackScope,
    ) -> R<MediaPlaybackBookmark> {
        let Storage { record, .. } = self.playback_record(scope)?;
        Ok(record
            .entries
            .into_iter()
            .find(|entry| entry.scope == *scope)
            .map_or(
                MediaPlaybackBookmark {
                    revision: "0".into(),
                    position: None,
                },
                |entry| entry.bookmark,
            ))
    }
    pub(crate) fn media_playback_save(
        &self,
        scope: &MediaPlaybackScope,
        expected: &str,
        position: MediaPlaybackPosition,
    ) -> R<MediaPlaybackBookmark> {
        if !position.valid() || !id(expected) {
            return Err(invalid());
        }
        let Storage {
            mut record,
            id: key_id,
            aad,
            key,
        } = self.playback_record(scope)?;
        let index = record
            .entries
            .iter()
            .position(|entry| entry.scope == *scope);
        let revision = index.map_or("0", |index| {
            record.entries[index].bookmark.revision.as_str()
        });
        if revision != expected {
            return Err(CoreError::Stale {
                reason: "Playback progress changed in another player.".into(),
            });
        }
        if let Some(index) = index {
            record.entries.remove(index);
        }
        let bookmark = MediaPlaybackBookmark {
            revision: types::new_id("playback")?,
            position: Some(position),
        };
        record.entries.push(Saved {
            scope: scope.clone(),
            bookmark: bookmark.clone(),
        });
        if record.entries.len() > MAX_ENTRIES {
            record.entries.remove(0);
        }
        let plain = Zeroizing::new(serde_json::to_vec(&record).map_err(|_| storage())?);
        if plain.len() > MAX_BYTES {
            return Err(invalid());
        }
        let encoded = crate::local_sealed::seal(&key, &aad, &plain).map_err(|()| storage())?;
        self.store.set_meta(&key_id, &encoded)?;
        Ok(bookmark)
    }
}

#[cfg(test)]
mod tests;
