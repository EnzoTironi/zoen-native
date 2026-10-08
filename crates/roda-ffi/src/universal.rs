//! Chat backgrounds (typed log event + content-addressed media) and universal search
//! (SQLite FTS5 index kept as a projection of the log).

use roda_types::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::apps;
use crate::dto::*;
use crate::engine::{now_ms, Engine, EntryBody, R};
use crate::i18n::t;
use crate::CoreError;

/// Largest attachment the core accepts (background photos are ~0.5 MB after resizing).
const MAX_MEDIA_BYTES: usize = 8 * 1024 * 1024;

impl Engine {
    // ── media ──

    /// Stores bytes under their sha256 and returns the reference to put in an event.
    pub fn put_media(
        &mut self,
        bytes: &[u8],
        mime: &str,
        width: u32,
        height: u32,
    ) -> R<MediaRefDto> {
        if bytes.is_empty() || bytes.len() > MAX_MEDIA_BYTES {
            return Err(CoreError::Invalid {
                reason: t(
                    "anexo vazio ou grande demais",
                    "attachment empty or too large",
                ),
            });
        }
        if !matches!(mime, "image/heic" | "image/jpeg" | "image/png") {
            return Err(CoreError::Invalid {
                reason: t("tipo de anexo não suportado", "unsupported attachment type"),
            });
        }
        let sha = hex::encode(Sha256::digest(bytes));
        self.store.put_media(&sha, mime, bytes, now_ms())?;
        Ok(MediaRefDto {
            sha256: sha,
            mime: mime.into(),
            width,
            height,
            bytes: bytes.len() as u64,
        })
    }

    /// The bytes for a hash, re-verified against it (a tampered blob reads as missing).
    pub fn media(&self, sha256: &str) -> R<Option<Vec<u8>>> {
        Ok(self
            .store
            .media(sha256)?
            .and_then(|(_, b)| (hex::encode(Sha256::digest(&b)) == sha256).then_some(b)))
    }

    /// Accepts bytes fetched from elsewhere only if they hash to `sha256`.
    pub fn import_media(&mut self, sha256: &str, bytes: &[u8], mime: &str) -> R<bool> {
        if hex::encode(Sha256::digest(bytes)) != sha256 {
            return Ok(false);
        }
        self.store.put_media(sha256, mime, bytes, now_ms())?;
        Ok(true)
    }

    // ── background ──

    pub fn set_background(&mut self, space: &str, background: &BackgroundDto) -> R<TimelineEntry> {
        let me = self.me_id()?;
        if !self.is_member(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros mudam o fundo",
                    "only members can change the background",
                ),
            });
        }
        let mut spec: BackgroundSpec = background.into();
        if spec.style == "photo" {
            let Some(m) = &spec.media else {
                return Err(CoreError::Invalid {
                    reason: t("foto sem anexo", "photo without attachment"),
                });
            };
            if self.store.media(&m.sha256)?.is_none() {
                return Err(CoreError::Invalid {
                    reason: t("anexo não encontrado", "attachment not found"),
                });
            }
        }
        let synced = self.net.synced.contains(space);
        if synced {
            // Everyone in the chat needs the photo: an encrypted copy goes to the relay.
            if let Some(m) = spec.media.as_mut() {
                self.seal_for_relay(m)?;
            }
        } else if let Some(m) = spec.media.as_mut() {
            (m.key, m.blob) = (None, None);
        }
        self.append(space, &me, EventBody::BackgroundSet { background: spec })?;
        let s = self.space_state(space)?;
        Ok(self.entry_dto(s.entries.last().expect("acabou de entrar")))
    }

    /// The chat's current shared background: the latest `BackgroundSet` in its log.
    pub fn background(&self, space: &str) -> R<Option<BackgroundDto>> {
        let s = self.space_state(space)?;
        Ok(s.entries.iter().rev().find_map(|e| match &e.body {
            EntryBody::Background { spec } => Some(spec.into()),
            _ => None,
        }))
    }

    // ── universal search ──

    fn reindex(&mut self) -> R<()> {
        self.store.index_begin()?;
        if let Err(e) = self.fill_index() {
            self.store.index_abort();
            return Err(e);
        }
        self.store.index_commit()?;
        self.index_dirty = false;
        Ok(())
    }

    fn fill_index(&self) -> R<()> {
        let st = &self.store;
        st.index_clear()?;
        let me = self.me.clone().unwrap_or_default();
        for id in &self.identity_order {
            if *id == me {
                continue;
            }
            let Some(i) = self.identities.get(id) else {
                continue;
            };
            let kind = if matches!(i.kind, IdentityKind::Agent) {
                "agent"
            } else {
                "person"
            };
            st.index_put(
                kind,
                &i.id,
                "",
                "",
                0,
                &i.name,
                &format!("@{} {}", i.handle, i.bio),
            )?;
        }
        for sid in &self.space_order {
            let Some(s) = self.state.spaces.get(sid) else {
                continue;
            };
            let names: Vec<String> = s
                .members
                .iter()
                .filter_map(|(m, _)| self.identities.get(m).map(|i| i.name.clone()))
                .collect();
            st.index_put(
                "space",
                &s.id,
                &s.id,
                "",
                s.last_at_ms,
                &s.title,
                &names.join(", "),
            )?;
            for e in &s.entries {
                let EntryBody::Message { text, .. } = &e.body else {
                    continue;
                };
                let body = searchable_text(text);
                if body.is_empty() {
                    continue;
                }
                let who = self
                    .identities
                    .get(&e.author)
                    .map(|i| i.name.clone())
                    .unwrap_or_default();
                st.index_put("message", e.id(), &s.id, &e.author, e.at_ms, &who, &body)?;
            }
        }
        for it in self.state.items.values() {
            let v = it.versions.last().expect("item sempre tem v1");
            let (kind, body) = match &v.content {
                ItemContent::Plan(p) => {
                    let mut b = p.summary.clone();
                    for l in p.sections.iter().flat_map(|s| &s.lines) {
                        b.push_str(" · ");
                        b.push_str(&l.text);
                    }
                    ("item", b)
                }
                ItemContent::Text { text } => ("item", text.clone()),
                ItemContent::App(a) => (
                    "app",
                    apps::headline(
                        &a.app,
                        &serde_json::from_str(&a.state_json).unwrap_or(Value::Null),
                        now_ms(),
                    )
                    .0,
                ),
            };
            st.index_put(
                kind,
                &it.id,
                &it.space,
                &v.author,
                v.at_ms,
                &v.content.title(),
                &body,
            )?;
        }
        Ok(())
    }

    /// Searches messages, people, agents, spaces, items and mini-apps on this device.
    /// `kinds` empty = all; `limit` per kind.
    pub fn universal_search(
        &mut self,
        query: &str,
        kinds: &[String],
        limit: u32,
    ) -> R<UniversalResults> {
        let started = std::time::Instant::now();
        if self.index_dirty {
            self.reindex()?;
        }
        let all = ["message", "person", "agent", "space", "app", "item"];
        let wanted: Vec<&str> = if kinds.is_empty() {
            all.to_vec()
        } else {
            all.iter()
                .copied()
                .filter(|k| kinds.iter().any(|w| w == k))
                .collect()
        };
        let mut hits = Vec::new();
        for kind in wanted {
            for r in self.store.search(query, kind, limit)? {
                let space_id = (!r.space.is_empty()).then(|| r.space.clone());
                let space_title = space_id
                    .as_ref()
                    .and_then(|s| self.state.spaces.get(s))
                    .map(|s| s.title.clone());
                let persona = match kind {
                    "person" | "agent" => Some(self.persona(&r.ref_id)),
                    _ if !r.author.is_empty() => Some(self.persona(&r.author)),
                    _ => None,
                };
                hits.push(UniversalHit {
                    kind: r.kind,
                    ref_id: r.ref_id,
                    space_id,
                    space_title,
                    title: r.title,
                    title_snippet: r.title_snippet,
                    snippet: r.snippet,
                    at_ms: r.at_ms,
                    persona,
                });
            }
        }
        Ok(UniversalResults {
            hits,
            took_us: started.elapsed().as_micros() as u64,
        })
    }
}

/// What a message contributes to the index: voice notes by their transcript, background
/// markers (old builds) not at all.
fn searchable_text(text: &str) -> String {
    if text.starts_with("⟦bg:") {
        return String::new();
    }
    if text.starts_with("⟦voice:") {
        return text
            .split_once('\n')
            .map(|(_, t)| t.trim().to_string())
            .unwrap_or_default();
    }
    text.to_string()
}
