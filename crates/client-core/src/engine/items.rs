//! Current/historical item views and ordinary restoration.
use super::*;

impl Engine {
    pub fn item(&self, id: &str) -> R<ItemDetail> {
        let it = self.item_state(id)?;
        let v = it.current();
        let snapshot = if it.kind == ItemKind::Plan {
            Some(self.plan_snapshot(id)?)
        } else {
            None
        };
        let space_title = self
            .state
            .spaces
            .get(&it.space)
            .map(|s| s.title.clone())
            .unwrap_or_default();
        let (plan, text) = if let Some(snapshot) = &snapshot {
            let total = snapshot.total_cents.ok_or_else(|| CoreError::Invalid {
                reason: t(
                    "o total do plano excede o limite; as linhas e o histórico foram preservados",
                    "the Plan total exceeds the limit; its lines and history are preserved",
                ),
            })?;
            (
                Some(plan_to_dto(&snapshot.document, total)),
                Some(snapshot.text.clone()),
            )
        } else {
            match &v.content {
                ItemContent::Plan(p) => {
                    let total = Self::plan_total(p).ok_or_else(|| CoreError::Invalid {
                        reason: t(
                            "total do plano fora do limite",
                            "Plan total exceeds the limit",
                        ),
                    })?;
                    (Some(plan_to_dto(p, total)), None)
                }
                ItemContent::Text { text } => (None, Some(text.clone())),
                ItemContent::App(_) => (None, None),
                ItemContent::Page(_) => (None, Some(self.page_text(id))),
                ItemContent::File(_) => (None, None),
            }
        };
        let (path, file) = match &v.content {
            ItemContent::Page(p) => (p.path.clone(), None),
            ItemContent::File(f) => (f.path.clone(), Some(self.file_dto(f))),
            _ => (String::new(), None),
        };
        let linked_requests = self
            .state
            .request_order
            .iter()
            .filter_map(|r| self.state.requests.get(r))
            .filter(|r| r.req.item.as_deref() == Some(id))
            .map(|r| self.request_dto(r))
            .collect();
        Ok(ItemDetail {
            id: it.id.clone(),
            space_id: it.space.clone(),
            space_title,
            kind_label: kind_label(it.kind).into(),
            kind_id: kind_id(it.kind).into(),
            title: snapshot
                .as_ref()
                .map_or_else(|| v.content.title(), |plan| plan.document.title.clone()),
            origin: it.origin.clone(),
            // A current-state share does not reveal/prove the original creator.
            created_by: self.persona(if it.shared.is_some() {
                ""
            } else {
                &it.versions[0].author
            }),
            plan,
            text,
            version: snapshot.as_ref().map_or(v.number, |plan| plan.version),
            versions: it
                .versions
                .iter()
                .rev()
                .map(|v| VersionDto {
                    number: v.number,
                    author: self.persona(&v.author),
                    at_ms: v.at_ms,
                    note: v.note.clone(),
                    is_undo: v.is_undo,
                    total_cents: match &v.content {
                        ItemContent::Plan(p) => Self::plan_total(p),
                        _ => None,
                    },
                })
                .collect(),
            linked_requests,
            app: self.app_state_dto(it),
            path,
            file,
        })
    }

    pub fn item_at(&self, id: &str, number: u32) -> R<ItemDetail> {
        let state = self.item_state(id)?;
        if state.kind == ItemKind::Plan {
            self.require_legacy_plan(id)?;
        }
        let position = state
            .versions
            .iter()
            .position(|v| v.number == number)
            .ok_or_else(|| not_found(&t("versão", "version")))?;
        let mut past = state.clone();
        past.versions.truncate(position + 1);
        let version = past.current();
        let mut detail = self.item(id)?;
        detail.version = version.number;
        detail.title = version.content.title();
        detail.plan = match &version.content {
            ItemContent::Plan(plan) => {
                let total = Self::plan_total(plan).ok_or_else(|| CoreError::Invalid {
                    reason: t(
                        "total do plano fora do limite",
                        "Plan total exceeds the limit",
                    ),
                })?;
                Some(plan_to_dto(plan, total))
            }
            _ => None,
        };
        detail.text = match &version.content {
            ItemContent::Text { text } => Some(text.clone()),
            ItemContent::Page(_) => Some(
                self.page_at(id, number)?
                    .blocks
                    .iter()
                    .map(|b| b.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        };
        (detail.path, detail.file) = match &version.content {
            ItemContent::Page(page) => (page.path.clone(), None),
            ItemContent::File(file) => (file.path.clone(), Some(self.file_dto(file))),
            _ => (String::new(), None),
        };
        detail.app = self.app_state_dto(&past);
        Ok(detail)
    }

    pub fn items(&self) -> Vec<ItemDetail> {
        let mut v: Vec<&ItemState> = self.state.items.values().collect();
        v.sort_by_key(|i| std::cmp::Reverse(i.current().at_ms));
        v.into_iter()
            .filter_map(|i| self.item(&i.id).ok())
            .collect()
    }

    pub fn restore_version(&mut self, item: &str, version: u32, note: &str) -> R<ItemDetail> {
        let me = self.me_id()?;
        let it = self.item_state(item)?;
        if it.kind == ItemKind::Plan {
            return Err(CoreError::Invalid {
                reason: t(
                    "a restauração de um plano exige o vínculo observado",
                    "a plan restore needs the observed binding",
                ),
            });
        }
        if !it.versions.iter().any(|v| v.number == version) {
            return Err(not_found(&t("versão", "version")));
        }
        if it.kind == ItemKind::Page {
            // A page's versions are Loro changes: restoring is a new change, not a copy.
            self.page_restore(item, version, note)?;
            return self.item(item);
        }
        let space = it.space.clone();
        self.append(
            &space,
            &me,
            EventBody::ItemReverted {
                item: item.into(),
                to_version: version,
                note: note.into(),
            },
        )?;
        self.item(item)
    }
}
