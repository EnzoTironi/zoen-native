//! Read-only pagination over one committed FTS generation. No result snapshot
//! or second history is persisted: edits require an explicit search restart.
use crate::{
    engine::{Engine, R},
    Client, CoreError, UniversalHit,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct UniversalSearchPage {
    pub hits: Vec<UniversalHit>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    binding: String,
    offset: u32,
}

fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "Busca inválida ou desatualizada. Reinicie a busca.".into(),
    }
}

impl Engine {
    fn universal_page(
        &mut self,
        query: &str,
        kinds: &[String],
        cursor: Option<&str>,
        limit: u32,
        identity: &str,
        device: &str,
    ) -> R<UniversalSearchPage> {
        if !self.is_unlocked()
            || self.me.as_deref() != Some(identity)
            || !self
                .account()
                .is_some_and(|account| account.identity == identity && account.device == device)
            || self.provider_setup_pending()
        {
            return Err(CoreError::Forbidden {
                reason: "A conta ou o dispositivo mudou.".into(),
            });
        }
        const ALL: [&str; 6] = ["agent", "app", "item", "message", "person", "space"];
        if !(1..=100).contains(&limit)
            || query.len() > 512
            || query.contains('\0')
            || kinds.len() > ALL.len()
            || kinds.iter().any(|kind| !ALL.contains(&kind.as_str()))
        {
            return Err(invalid());
        }
        let wanted: Vec<String> = ALL
            .iter()
            .filter(|kind| kinds.is_empty() || kinds.iter().any(|want| want == **kind))
            .map(|kind| (*kind).into())
            .collect();
        let mut spaces: Vec<String> = self
            .state
            .spaces
            .keys()
            .filter(|space| self.is_member(space, identity))
            .cloned()
            .collect();
        spaces.sort();
        if self.index_dirty {
            self.reindex()?;
        }
        let generation = self.store.search_generation()?;
        let binding = hex::encode(Sha256::digest(
            serde_json::to_vec(&(1_u8, query, &wanted, identity, device, &spaces, generation))
                .map_err(|_| invalid())?,
        ));
        let offset = cursor
            .map(|value| {
                if value.len() > 4096 {
                    return Err(invalid());
                }
                let position: Cursor = serde_json::from_str(value).map_err(|_| invalid())?;
                if position.binding != binding {
                    return Err(invalid());
                }
                Ok(position.offset)
            })
            .transpose()?
            .unwrap_or_default();
        let mut rows = self
            .store
            .universal_search_page(query, &wanted, &spaces, offset, limit)?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor = if more {
            Some(
                serde_json::to_string(&Cursor {
                    binding,
                    offset: offset.checked_add(limit).ok_or_else(invalid)?,
                })
                .map_err(|_| invalid())?,
            )
        } else {
            None
        };
        let hits = rows
            .into_iter()
            .map(|row| {
                let space_id = (!row.space.is_empty()).then_some(row.space);
                let space_title = space_id
                    .as_ref()
                    .and_then(|space| self.state.spaces.get(space))
                    .map(|space| space.title.clone());
                let persona = match row.kind.as_str() {
                    "agent" | "person" => Some(self.persona(&row.ref_id)),
                    _ if !row.author.is_empty() => Some(self.persona(&row.author)),
                    _ => None,
                };
                UniversalHit {
                    kind: row.kind,
                    ref_id: row.ref_id,
                    space_id,
                    space_title,
                    title: row.title,
                    title_snippet: row.title_snippet,
                    snippet: row.snippet,
                    at_ms: row.at_ms,
                    persona,
                }
            })
            .collect();
        Ok(UniversalSearchPage { hits, next_cursor })
    }
}

#[uniffi::export]
impl Client {
    /// Total limit across kinds, relevance then date and immutable identity.
    /// Treat the cursor as opaque; a mutation or access change requires restart.
    pub fn universal_search_page(
        &self,
        query: String,
        kinds: Vec<String>,
        cursor: Option<String>,
        limit: u32,
        expected_identity_id: String,
        expected_device_id: String,
    ) -> Result<UniversalSearchPage, CoreError> {
        self.lock().universal_page(
            &query,
            &kinds,
            cursor.as_deref(),
            limit,
            &expected_identity_id,
            &expected_device_id,
        )
    }
}
