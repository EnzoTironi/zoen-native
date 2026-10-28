//! Item and persona display projections over accepted document versions.
use super::{
    json, plan_value, t, total, ChunkRef, Client, Item, ItemContent, ItemKind, Signer, Value,
};

impl<S: Signer> Client<S> {
    pub(in crate::client) fn persona_value(&self, id: &str) -> Value {
        let profile = self.saved.profiles.get(id);
        let name = profile
            .map(|profile| profile.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                if id == self.account.identity {
                    self.account.name.clone()
                } else if let Some(profile) = profile.filter(|profile| !profile.handle.is_empty()) {
                    format!("@{}", profile.handle)
                } else {
                    "Desconhecido".into()
                }
            });
        let owner = profile.and_then(|profile| profile.owner.as_ref());
        let owner_profile = owner.and_then(|id| self.saved.profiles.get(id));
        let photo = self.fields_of(id).and_then(|(_, fields)| fields.photo);
        json!({ "id": id, "kind": profile.map_or(types::IdentityKind::Person, |profile| profile.kind),
            "name": name, "handle": profile.map_or(if id==self.account.identity {&self.account.handle}else{""}, |profile| profile.handle.as_str()),
            "initials": name.split_whitespace().take(2).filter_map(|part| part.chars().next()).collect::<String>().to_uppercase(),
            "tint_hex": profile.map_or("#8E8E93", |profile| profile.tint_hex.as_str()),
            "glyph": profile.and_then(|profile| profile.glyph.as_ref()), "bio": profile.map_or("", |profile| profile.bio.as_str()),
            "owner_id": owner, "owner_name": owner_profile.map(|profile| &profile.name),
            "owner_tint_hex": owner_profile.map(|profile| &profile.tint_hex),
            "photo_sha256": photo.as_ref().map(|photo| &photo.sha256), "photo_mime": photo.as_ref().map(|photo| &photo.mime),
            "photo_ready": photo.as_ref().is_some_and(|photo| self.profile_photo_cached(photo)),
            "is_me": id == self.account.identity, "is_mine": owner.is_some_and(|owner| *owner == self.account.identity) })
    }

    pub(in crate::client) fn item_value(
        &self,
        item: &Item,
        at: Option<u32>,
        downloads: &mut Vec<ChunkRef>,
    ) -> Result<Value, String> {
        let mut selected = item.clone();
        if let Some(version) = at {
            if version == 0 || version as usize > item.versions.len() {
                return Err("Versão indisponível.".into());
            }
            selected.versions.truncate(version as usize)?;
        }
        let current = selected.current();
        let mut plan = None;
        let mut text = None;
        let mut path = String::new();
        let mut file = None;
        let mut title = current.content.title();
        if item.kind == ItemKind::Plan {
            let read = self.plan_read(&selected)?;
            downloads.extend(read.downloads.clone());
            title.clone_from(&read.document.title);
            plan = Some(plan_value(&read.document)?);
        }
        match &current.content {
            ItemContent::Text { text: value } => text = Some(value.clone()),
            ItemContent::Page(content) if item.kind == ItemKind::Page => {
                let read = self.read_page(&selected, at.is_none())?;
                downloads.extend(read.downloads.clone());
                text = Some(
                    read.live
                        .blocks()
                        .iter()
                        .map(::documents::Block::plain_text)
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                path.clone_from(&content.path);
            }
            ItemContent::Page(_) if item.kind == ItemKind::Plan => {}
            ItemContent::File(document) => {
                path.clone_from(&document.path);
                file = Some(json!({
                    "name": document.name, "path": document.path, "mime": document.mime,
                    "bytes": document.bytes.to_string(), "sha256": document.sha256,
                    "ready": document.chunks.iter().all(|chunk| self.saved.local_chunks.contains_key(&chunk.blob)),
                    "chunks": document.chunks.len(),
                    "chunks_here": document.chunks.iter().filter(|chunk| self.saved.local_chunks.contains_key(&chunk.blob)).count(),
                    "has_thumbnail": document.thumb.is_some(),
                }));
            }
            _ => {}
        }
        let (kind_id, kind_label) = match item.kind {
            ItemKind::Plan => ("plan", t("Plano", "Plan")),
            ItemKind::Task => ("task", t("Tarefa", "Task")),
            ItemKind::Note => ("note", t("Nota", "Note")),
            ItemKind::Page => ("page", t("Página", "Page")),
            ItemKind::File => ("file", t("Arquivo", "File")),
            ItemKind::App => ("app", t("App", "App")),
        };
        Ok(json!({ "id": item.id, "space_id": item.space,
            "space_title": self.metadata(&item.space).map(|metadata| metadata.0).unwrap_or_default(),
            "kind_id": kind_id, "kind_label": kind_label, "title": title, "origin": item.origin,
            "created_by": self.persona_value(&item.versions.first().event.author), "plan": plan, "text": text,
            "version": selected.versions.len(), "versions": item.versions.iter().enumerate().rev().map(|(index, version)| json!({
                "number": index + 1, "author": self.persona_value(&version.event.author),
                "at_ms": version.event.at_ms.to_string(), "note": version.note, "is_undo": version.is_undo,
                "total_cents": match &version.content { ItemContent::Plan(document) => total(document).map(|total| total.to_string()), _ => None },
            })).collect::<Vec<_>>(), "linked_requests": [], "app": self.app_state_value(&selected,self.view_now_ms.max(selected.current().event.at_ms))?, "path": path, "file": file }))
    }

    pub(in crate::client) fn item_card_value(
        &self,
        space: &str,
        id: &str,
    ) -> Result<Option<Value>, String> {
        let item = self.document_item(id)?;
        if item.space != space {
            return Ok(None);
        }
        let detail = self.item_value(&item, None, &mut Vec::new())?;
        let plan = detail["plan"].as_object();
        let lines = plan
            .and_then(|plan| plan.get("sections"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|section| section["lines"].as_array())
            .flatten()
            .collect::<Vec<_>>();
        Ok(Some(
            json!({"item_id":id,"title":detail["title"],"summary":plan.and_then(|plan|plan.get("summary")).and_then(Value::as_str)
            .or_else(||detail["app"]["headline"].as_str()).or_else(||detail["text"].as_str()).unwrap_or(""),
            "kind_id":detail["kind_id"],"kind_label":detail["kind_label"],"version":detail["version"],
            "total_cents":plan.and_then(|plan|plan.get("total_cents")),"budget_cents":plan.and_then(|plan|plan.get("budget_cents")),
            "line_count":lines.len(),"done_count":lines.iter().filter(|line|line["done"]==true).count(),"app":detail["app"]}),
        ))
    }
}
