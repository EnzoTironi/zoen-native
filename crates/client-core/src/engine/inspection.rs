//! Integrity reports, search, mentions and aggregate statistics.
use super::{
    apps, event_label, fold, money, not_found, now_ms, CoreStats, Engine, EntryBody, ItemContent,
    ItemKind, LogError, LogEventDto, LogReport, Mention, SearchHit, SpaceKind, SpaceLog, Value, R,
};

impl Engine {
    // ── integridade, busca, menções ──

    pub fn verify_log(&self, space: &str) -> LogReport {
        let title = self
            .space_summary(space)
            .map(|s| s.title)
            .or_else(|_| self.space_state(space).map(|s| s.title.clone()))
            .unwrap_or_default();
        // Reverifica a partir do DISCO, não da memória: pega adulteração no arquivo.
        let result: Result<SpaceLog, String> = self
            .store
            .events(space)
            .map_err(|e| e.to_string())
            .and_then(|events| {
                SpaceLog::from_events(space.to_string(), events)
                    .map_err(|e: LogError| e.to_string())
            });
        match result {
            Ok(log) => LogReport {
                space_id: space.into(),
                space_title: title,
                events: log.len() as u64,
                head_hash: log.head_hash().into(),
                valid: true,
                error: None,
            },
            Err(err) => LogReport {
                space_id: space.into(),
                space_title: title,
                events: 0,
                head_hash: String::new(),
                valid: false,
                error: Some(err),
            },
        }
    }

    pub fn verify_all(&self) -> Vec<LogReport> {
        self.space_order
            .iter()
            .map(|s| self.verify_log(s))
            .collect()
    }

    pub fn log_events(&self, space: &str) -> R<Vec<LogEventDto>> {
        let log = self.logs.get(space).ok_or_else(|| not_found("log"))?;
        Ok(log
            .events()
            .iter()
            .rev()
            .map(|e| LogEventDto {
                seq: e.seq,
                label: event_label(&e.body),
                author: self.persona(&e.author),
                at_ms: e.at_ms,
                hash: e.hash.clone(),
                prev: e.prev.clone(),
                signature: e.sig.clone(),
            })
            .collect())
    }

    pub fn search(&self, query: &str) -> Vec<SearchHit> {
        let q = fold(query.trim());
        if q.is_empty() {
            return vec![];
        }
        let mut hits = Vec::new();
        for it in self.state.items.values() {
            let v = it.current();
            let plan = if it.kind == ItemKind::Plan {
                match self.plan_snapshot(&it.id) {
                    Ok(plan) => Some(plan),
                    Err(_) => continue,
                }
            } else {
                None
            };
            let title = plan
                .as_ref()
                .map_or_else(|| v.content.title(), |plan| plan.document.title.clone());
            let mut hay = plan
                .as_ref()
                .map_or_else(|| title.clone(), |plan| plan.text.clone());
            if let ItemContent::Plan(p) = &v.content {
                if plan.is_none() {
                    for l in p.sections.iter().flat_map(|s| &s.lines) {
                        hay.push_str(" · ");
                        hay.push_str(&l.text);
                    }
                }
            }
            if fold(&hay).contains(&q) {
                let snippet = if let Some(plan) = &plan {
                    plan.document
                        .sections
                        .iter()
                        .flat_map(|section| &section.lines)
                        .find(|line| fold(&line.text).contains(&q))
                        .map_or_else(
                            || plan.text.clone(),
                            |line| format!("{} · {}", line.text, money(line.cost_cents)),
                        )
                } else {
                    match &v.content {
                        ItemContent::Plan(p) => p
                            .sections
                            .iter()
                            .flat_map(|s| &s.lines)
                            .find(|l| fold(&l.text).contains(&q))
                            .map(|l| format!("{} · {}", l.text, money(l.cost_cents)))
                            .unwrap_or(p.summary.clone()),
                        ItemContent::Text { text } => text.clone(),
                        ItemContent::Page(_) => self.page_text(&it.id),
                        ItemContent::File(f) => f.name.clone(),
                        ItemContent::App(a) => {
                            apps::headline(
                                &a.app,
                                &serde_json::from_str(&a.state_json).unwrap_or(Value::Null),
                                now_ms(),
                            )
                            .0
                        }
                    }
                };
                hits.push(SearchHit {
                    space_id: it.space.clone(),
                    space_title: self
                        .space_summary(&it.space)
                        .map(|s| s.title)
                        .unwrap_or_default(),
                    item_id: Some(it.id.clone()),
                    title,
                    snippet,
                    author: self.persona(
                        plan.as_ref()
                            .map_or(v.author.as_str(), |plan| plan.author.as_str()),
                    ),
                    at_ms: plan.as_ref().map_or(v.at_ms, |plan| plan.at_ms),
                });
            }
        }
        for s in self
            .state
            .spaces
            .values()
            .filter(|s| s.kind != SpaceKind::Personal)
        {
            for e in &s.entries {
                if let EntryBody::Message { text, .. } = &e.body {
                    let snippet = if fold(text).contains(&q) {
                        Some(text.clone())
                    } else {
                        self.entry_voice(e)
                            .filter(|voice| fold(&voice.transcript).contains(&q))
                            .map(|voice| voice.transcript)
                    };
                    if let Some(snippet) = snippet {
                        hits.push(SearchHit {
                            space_id: s.id.clone(),
                            space_title: self
                                .space_summary(&s.id)
                                .map(|x| x.title)
                                .unwrap_or_default(),
                            item_id: None,
                            title: self.persona(&e.author).name,
                            snippet,
                            author: self.persona(&e.author),
                            at_ms: e.at_ms,
                        });
                    }
                }
            }
        }
        hits.sort_by(|a, b| {
            b.item_id
                .is_some()
                .cmp(&a.item_id.is_some())
                .then(b.at_ms.cmp(&a.at_ms))
        });
        hits.truncate(40);
        hits
    }

    pub fn mentions(&self) -> Vec<Mention> {
        let Some(me) = self.me.clone() else {
            return vec![];
        };
        let handle = format!("@{}", self.persona(&me).name.to_lowercase());
        let mut out = Vec::new();
        for s in self.state.spaces.values() {
            for e in &s.entries {
                if e.author == me {
                    continue;
                }
                if let EntryBody::Message { text, .. } = &e.body {
                    if text.to_lowercase().contains(&handle) {
                        out.push(Mention {
                            space_id: s.id.clone(),
                            space_title: self
                                .space_summary(&s.id)
                                .map(|x| x.title)
                                .unwrap_or_default(),
                            entry: self.entry_dto(e),
                        });
                    }
                }
            }
        }
        out.sort_by_key(|x| std::cmp::Reverse(x.entry.at_ms));
        out
    }

    pub fn stats(&self) -> CoreStats {
        CoreStats {
            spaces: self
                .state
                .spaces
                .values()
                .filter(|s| s.kind != SpaceKind::Personal)
                .count() as u32,
            events: self.store.event_count().unwrap_or(0),
            items: self.state.items.len() as u32,
            identities: self.identities.len() as u32,
            all_logs_valid: self.verify_all().iter().all(|r| r.valid),
            db_path: self.db_path.clone(),
        }
    }
}
