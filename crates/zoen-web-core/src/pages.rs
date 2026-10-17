//! Browser live-page editing uses the same Loro document and signed MLS event path as
//! native. The host must persist the returned Update before transmitting its frames.
use crate::{Client, Update};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use openmls_traits::signatures::Signer;
use roda_docs::{Block, BlockEdit, Frontiers, Kind, Page, Span};
use roda_types::{EventBody, ItemContent, PageContent, Role};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Enforce announcement authorship while replaying, not just in the editor UI.
pub(crate) fn authorized_page_events(events: Vec<&roda_types::Event>) -> Vec<&roda_types::Event> {
    let mut roles = std::collections::BTreeMap::new();
    let mut announced = std::collections::BTreeSet::new();
    events
        .into_iter()
        .filter(|event| {
            let key = |id: &str| (event.space.clone(), id.to_owned());
            let admin = matches!(
                roles.get(&key(&event.author)),
                Some(Role::Owner | Role::Admin)
            );
            match &event.body {
                EventBody::SpaceCreated { .. } => {
                    roles.insert(key(&event.author), Role::Owner);
                }
                EventBody::MemberAdded { identity, role } => {
                    roles.insert(key(identity), *role);
                }
                EventBody::MemberRemoved { identity } => {
                    roles.remove(&key(identity));
                }
                EventBody::MessagePosted { text, .. } if admin => {
                    if let Some(raw) = text.strip_prefix("zoen-living-space-meta-v1:") {
                        if let Ok(value) = serde_json::from_str::<Value>(raw) {
                            if value["action"] == "announce" {
                                if let Some(id) = value["item_id"].as_str() {
                                    announced.insert(key(id));
                                }
                            }
                        }
                    }
                }
                EventBody::ItemVersioned { item, .. }
                    if announced.contains(&key(item)) && !admin =>
                {
                    return false
                }
                _ => {}
            }
            true
        })
        .collect()
}

fn block_dto(block: &Block) -> Value {
    let kind = serde_json::to_value(&block.kind).expect("kind serializes");
    json!({"id":block.id,"kind":kind["k"],"level":kind["level"].as_u64().unwrap_or(0),"indent":kind["indent"].as_u64().unwrap_or(0),"number":kind["number"].as_u64().unwrap_or(0),"checked":kind["checked"].as_bool().unwrap_or(false),"lang":kind["lang"].as_str().unwrap_or(""),"url":kind["url"].as_str().unwrap_or(""),"alt":kind["alt"].as_str().unwrap_or(""),"text":block.plain_text(),"spans":roda_docs::spans_of(&block.runs)})
}
#[derive(Deserialize)]
struct EditContext {
    item: String,
    frontiers: String,
    content_hash: String,
}
#[derive(Deserialize)]
struct SaveRequest {
    item_id: String,
    edit_id: String,
    edit_context: String,
    base_blocks: Vec<Value>,
    blocks: Vec<Value>,
    order: Vec<String>,
    changed: Vec<Value>,
}

#[derive(Serialize)]
pub struct PageCreated {
    pub update: Update,
    pub result: Value,
}

fn edit(block: &Value) -> Result<BlockEdit, String> {
    let id = block["id"].as_str().ok_or("Block ID required")?.to_owned();
    let mut kind = block.clone();
    kind["k"] = block["kind"].clone();
    let parsed: Kind = serde_json::from_value(kind).map_err(|e| e.to_string())?;
    let spans = block["spans"]
        .as_array()
        .ok_or("Formatting ranges required")?
        .iter()
        .map(|s| {
            Ok(Span {
                start: u32::try_from(s["start"].as_u64().ok_or("Invalid mark start")?)
                    .map_err(|_| "Invalid mark start")?,
                end: u32::try_from(s["end"].as_u64().ok_or("Invalid mark end")?)
                    .map_err(|_| "Invalid mark end")?,
                key: s["key"].as_str().ok_or("Invalid mark")?.into(),
                value: s["value"].as_str().ok_or("Invalid mark value")?.into(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(BlockEdit {
        id,
        kind: parsed,
        text: block["text"].as_str().ok_or("Text required")?.into(),
        spans,
    })
}

impl<S: Signer> Client<S> {
    pub fn page_create(
        &mut self,
        space: &str,
        title: &str,
        now_ms: i64,
        edit_id: &str,
    ) -> Result<PageCreated, String> {
        if title.len() > 255 || edit_id.trim().is_empty() || edit_id.len() > 128 {
            return Err("Invalid page title or edit ID".into());
        }
        let canonical=json!({"schema":"browser-page-create-v1","space":space,"title":title.trim(),"actor":self.living_identity()}).to_string();
        let digest = hex::encode(Sha256::digest(canonical.as_bytes()));
        let item = format!(
            "it_browser_{}",
            &hex::encode(Sha256::digest(
                format!("{space}:{}:{edit_id}", self.living_identity()).as_bytes()
            ))[..32]
        );
        let page = Page::new();
        let mut edits = vec![];
        if !title.trim().is_empty() {
            edits.push(BlockEdit {
                id: format!("{}_title", item),
                kind: Kind::Heading {
                    level: 1,
                    setext: false,
                },
                text: title.trim().into(),
                spans: vec![],
            });
        }
        edits.push(BlockEdit {
            id: format!("{}_body", item),
            kind: Kind::Paragraph,
            text: String::new(),
            spans: vec![],
        });
        let order = edits.iter().map(|b| b.id.clone()).collect::<Vec<_>>();
        page.apply(&order, &edits).map_err(|e| e.to_string())?;
        let content = PageContent {
            title: page.title(),
            path: String::new(),
            loro: STANDARD.encode(page.snapshot()),
            blob: None,
            manifest: None,
        };
        let update = self.queue_page_body(
            space,
            EventBody::ItemCreated {
                item: item.clone(),
                kind: roda_types::ItemKind::Page,
                content: ItemContent::Page(content),
                origin: "Created in browser".into(),
            },
            now_ms,
            edit_id,
            &digest,
        )?;
        Ok(PageCreated {
            update,
            result: self.page(&item)?,
        })
    }

    fn browser_page_chunks(&self, item: &str) -> Result<Vec<roda_types::ChunkRef>, String> {
        let mut space = None;
        let mut chunks = std::collections::BTreeMap::new();
        for event in self.page_events() {
            let content = match &event.body {
                EventBody::ItemCreated {
                    item: id,
                    content: ItemContent::Page(content),
                    ..
                } if id == item => {
                    if space.is_some() {
                        return Err("Ambiguous page ID".into());
                    }
                    space = Some(event.space.clone());
                    content
                }
                EventBody::ItemVersioned {
                    item: id,
                    content: ItemContent::Page(content),
                    ..
                } if id == item && space.as_deref() == Some(&event.space) => content,
                _ => continue,
            };
            if let Some(blob) = &content.blob {
                chunks.insert(blob.blob.clone(), blob.clone());
            }
        }
        let space = space.ok_or("Page not found")?;
        if self.page_role(&space).is_none() || !self.living_tier_allows(&space) {
            return Err("Page membership and entitlement required".into());
        }
        if chunks.len() > 512
            || chunks
                .values()
                .try_fold(0u64, |total, c| total.checked_add(c.bytes))
                .is_none_or(|total| total > 64 << 20)
        {
            return Err("Page history exceeds browser download limit".into());
        }
        Ok(chunks.into_values().collect())
    }
    pub fn page_download_manifest(&self, item: &str) -> Result<Value, String> {
        let missing: Vec<_> = self
            .browser_page_chunks(item)?
            .into_iter()
            .filter(|c| self.page_blob_get(&c.blob).is_none())
            .map(|c| json!({"blob":c.blob,"bytes":c.bytes,"sealed_bytes":c.bytes+40}))
            .collect();
        Ok(json!({"item":item,"ready":missing.is_empty(),"chunks":missing}))
    }
    pub fn page_blob_supply(
        &mut self,
        item: &str,
        blob: &str,
        sealed: &[u8],
    ) -> Result<(), String> {
        let reference = self
            .browser_page_chunks(item)?
            .into_iter()
            .find(|c| c.blob == blob)
            .ok_or("Blob does not belong to this accessible page")?;
        let plain = crate::files::open_chunk(&reference, sealed)?;
        self.page_blob_put(blob.into(), plain);
        Ok(())
    }

    fn browser_page(&self, item: &str) -> Result<(String, PageContent, u32, Page), String> {
        let page = Page::new();
        let mut current = None;
        let mut space = None;
        let mut version = 0;
        for event in self.page_events() {
            let content = match &event.body {
                EventBody::ItemCreated {
                    item: id,
                    content: ItemContent::Page(content),
                    ..
                } if id == item => {
                    if space.is_some() {
                        return Err("Ambiguous page ID".into());
                    }
                    space = Some(event.space.clone());
                    content
                }
                EventBody::ItemVersioned {
                    item: id,
                    content: ItemContent::Page(content),
                    ..
                } if id == item && space.as_deref() == Some(&event.space) => content,
                _ => continue,
            };
            if let Some(blob) = &content.blob {
                let bytes = self
                    .page_blob_get(&blob.blob)
                    .ok_or("Page blob downloads are pending")?;
                page.import(bytes).map_err(|e| e.to_string())?;
            }
            if content.manifest.is_some() {
                return Err("This typed page is not a binary document".into());
            }
            if !content.loro.is_empty() {
                page.import(&STANDARD.decode(&content.loro).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            }
            current = Some(content.clone());
            version += 1;
        }
        let space = space.ok_or("Page not found")?;
        if self.page_role(&space).is_none() || !self.living_tier_allows(&space) {
            return Err("Current Space entitlement and membership required".into());
        }
        Ok((space, current.ok_or("Page not found")?, version, page))
    }
    pub fn page(&self, item: &str) -> Result<Value, String> {
        let (space, content, version, page) = self.browser_page(item)?;
        let announced = self.living_space_home(&space).ok().is_some_and(|h| {
            h["announcement_items"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == item))
        });
        let can_edit = matches!(self.page_role(&space), Some(Role::Owner | Role::Admin))
            || (!announced && self.page_role(&space) == Some(Role::Member));
        let context=json!({"item":item,"frontiers":hex::encode(page.frontiers().encode()),"content_hash":page.content_hash()}).to_string();
        let error = self.page_save_error(item).map(str::to_owned).or_else(|| {
            self.page_draft(item)
                .filter(|(_, e)| !e.is_empty())
                .map(|(_, e)| e.clone())
        });
        let draft = error
            .as_ref()
            .and_then(|_| self.page_draft(item))
            .and_then(|(raw, _)| serde_json::from_str::<Value>(raw).ok());
        Ok(
            json!({"draft":draft,"item_id":item,"space_id":space,"title":page.title(),"path":content.path,"version":version,"ready":true,"unsaved":error.is_some(),"pending_sync":self.page_pending(item),"save_error":error,"can_edit":can_edit,"content_hash":page.content_hash(),"edit_context":context,"blocks":page.blocks().iter().map(block_dto).collect::<Vec<_>>()}),
        )
    }
    pub fn page_apply(&mut self, request_json: &str, now_ms: i64) -> Result<Update, String> {
        let result = self.page_apply_inner(request_json, now_ms);
        if request_json.len() <= 2_000_000 {
            if let Ok(request) = serde_json::from_str::<SaveRequest>(request_json) {
                // Preserve the original context and complete user input for retry, including
                // synchronous failures. Hosts persist the next Update before navigation.
                self.page_store_draft(
                    request.item_id,
                    request_json.into(),
                    result.as_ref().err().cloned().unwrap_or_default(),
                );
            }
        }
        result.and_then(|update| self.update(update.frames))
    }
    fn page_apply_inner(&mut self, request_json: &str, now_ms: i64) -> Result<Update, String> {
        if request_json.len() > 2_000_000 {
            return Err("Page edit is too large".into());
        }
        let request: SaveRequest = serde_json::from_str(request_json).map_err(|e| e.to_string())?;
        if request.edit_id.trim().is_empty() || request.edit_id.len() > 128 {
            return Err("Edit ID required".into());
        }
        let (space, mut content, _, current) = self.browser_page(&request.item_id)?;
        // Announcement edits require admin in every adapter, not merely hidden controls.
        if !matches!(self.page_role(&space), Some(Role::Owner | Role::Admin))
            && self.living_space_home(&space).ok().is_some_and(|h| {
                h["announcement_items"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == &request.item_id))
            })
        {
            return Err("Announcements are administrator-only".into());
        }
        let context: EditContext =
            serde_json::from_str(&request.edit_context).map_err(|_| "Invalid editor context")?;
        if context.item != request.item_id {
            return Err("Editor context belongs to another page".into());
        }
        let frontiers = Frontiers::decode(
            &hex::decode(context.frontiers).map_err(|_| "Invalid editor context")?,
        )
        .map_err(|e| e.to_string())?;
        let observed = current.at(&frontiers).map_err(|e| e.to_string())?;
        if observed.content_hash() != context.content_hash
            || observed.blocks().iter().map(block_dto).collect::<Vec<_>>() != request.base_blocks
        {
            return Err("Editor base does not match observed document".into());
        }
        let before = observed.version();
        let changed = request
            .changed
            .iter()
            .map(edit)
            .collect::<Result<Vec<_>, _>>()?;
        if request
            .blocks
            .iter()
            .map(|b| b["id"].as_str())
            .collect::<Vec<_>>()
            != request
                .order
                .iter()
                .map(|id| Some(id.as_str()))
                .collect::<Vec<_>>()
        {
            return Err("Block order does not match editor blocks".into());
        }
        observed
            .apply(&request.order, &changed)
            .map_err(|e| e.to_string())?;
        let delta = observed.updates_since(&before);
        current.import(&delta).map_err(|e| e.to_string())?;
        content.title = current.title();
        content.loro = STANDARD.encode(&delta);
        content.blob = None;
        let digest = hex::encode(Sha256::digest(request_json.as_bytes()));
        self.page_prepare_retry(&request.item_id, &request.edit_id, &digest)?;
        self.queue_page_body(
            &space,
            EventBody::ItemVersioned {
                item: request.item_id,
                content: ItemContent::Page(content),
                note: "Edited in browser".into(),
            },
            now_ms,
            &request.edit_id,
            &digest,
        )
    }
}
