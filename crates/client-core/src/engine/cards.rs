//! Item cards and message/voice presentation projections.
use super::{
    content_hash, kind_id, kind_label, t, ts, Delivery, Engine, Entry, EntryBody, EntryKind,
    EventBody, ItemCard, ItemContent, ItemKind, ItemState, MessageMarks, MessageMediaDto, Persona,
    PlanLine, ReactionDto, ReqStatus, SpaceState, TimelineEntry, VoiceMeta, VoiceNoteDto, R,
};

impl Engine {
    pub(super) fn line_hash(line: &PlanLine) -> R<String> {
        Ok(content_hash(&(&line.text, line.cost_cents))?)
    }

    // ── conversões para DTO ──

    pub(crate) fn card(&self, item: &str) -> Option<ItemCard> {
        let it = self.state.items.get(item)?;
        let plan = if it.kind == ItemKind::Plan {
            Some(self.plan_snapshot(item).ok()?)
        } else {
            None
        };
        self.item_card(it, plan)
    }

    pub(crate) fn card_in_space(&self, space: &str, item: &str) -> Option<ItemCard> {
        if self
            .state
            .items
            .get(item)
            .is_some_and(|it| it.space == space)
            && !self.logs.values().any(|log| {
                log.events().iter().any(|event| {
                event.space != space
                    && matches!(&event.body, EventBody::ItemCreated { item: id, .. } if id == item)
            })
            })
        {
            return self.card(item);
        }
        let it = self.space_projections.get(space)?.items.get(item)?;
        let plan = if it.kind == ItemKind::Plan {
            Some(self.plan_snapshot_in_space(space, item).ok()?)
        } else {
            None
        };
        self.item_card(it, plan)
    }

    pub(super) fn item_card(
        &self,
        it: &ItemState,
        plan: Option<crate::pages::PlanSnapshot>,
    ) -> Option<ItemCard> {
        let item = it.id.as_str();
        let v = it.current();
        let title = plan
            .as_ref()
            .map_or_else(|| v.content.title(), |plan| plan.document.title.clone());
        let version = plan.as_ref().map_or(v.number, |plan| plan.version);
        let document = plan.as_ref().map(|plan| &plan.document).or({
            if let ItemContent::Plan(document) = &v.content {
                Some(document)
            } else {
                None
            }
        });
        let (summary, total, budget, lines, done) = if let Some(p) = document {
            let all: Vec<&PlanLine> = p.sections.iter().flat_map(|s| &s.lines).collect();
            (
                p.summary.clone(),
                plan.as_ref()
                    .map_or_else(|| Self::plan_total(p), |plan| plan.total_cents),
                p.budget_cents,
                all.len() as u32,
                all.iter().filter(|l| l.done).count() as u32,
            )
        } else {
            match &v.content {
                ItemContent::Plan(_) => return None,
                ItemContent::Text { text } => (text.clone(), None, None, 0, 0),
                ItemContent::App(_) => (String::new(), None, None, 0, 0),
                ItemContent::Page(_) => {
                    let text = self.page_text(item);
                    let summary: String = text
                        .lines()
                        .skip(1)
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("")
                        .chars()
                        .take(160)
                        .collect();
                    (summary, None, None, 0, 0)
                }
                ItemContent::File(f) => (f.mime.clone(), None, None, 0, 0),
            }
        };
        Some(ItemCard {
            app: self.app_state_dto(it),
            item_id: it.id.clone(),
            title,
            summary,
            kind_label: kind_label(it.kind).into(),
            kind_id: kind_id(it.kind).into(),
            version,
            total_cents: total,
            budget_cents: budget,
            line_count: lines,
            done_count: done,
        })
    }

    /// The voice note an entry carries, if its attached Item is one (in this Space).
    pub(crate) fn voice_of(&self, space: &str, item: &str) -> Option<VoiceNoteDto> {
        let it = self.state.items.get(item).filter(|it| it.space == space)?;
        let ItemContent::File(doc) = &it.current().content else {
            return None;
        };
        if !doc.is_voice() {
            return None;
        }
        let meta = doc.voice.clone().unwrap_or_default();
        Some(VoiceNoteDto {
            item_id: item.to_string(),
            sha256: doc.sha256.clone(),
            mime: doc.mime.clone(),
            ms: meta.ms,
            levels: meta.bars(),
            transcript: meta.transcript,
        })
    }

    pub(super) fn entry_voice(&self, e: &Entry) -> Option<VoiceNoteDto> {
        let EntryBody::Message {
            text,
            attaches,
            marks,
            ..
        } = &e.body
        else {
            return None;
        };
        if marks.deleted {
            return None;
        }
        if let Some(item) = attaches {
            return self.voice_of(&e.space, item);
        }
        // File-backed markers need the current signed hash too. Markers without a
        // core file keep using the clients' local-only compatibility path.
        let (header, transcript) = text.strip_prefix("⟦voice:")?.split_once('⟧')?;
        let mut fields = header.split(':');
        let item = fields.next()?;
        let ms = fields.next()?.parse().ok()?;
        let levels = fields.next()?;
        if fields.next().is_some() {
            return None;
        }
        let mut voice = self.voice_of(&e.space, item)?;
        let ItemContent::File(original) = &self.state.items.get(item)?.versions.initial().content
        else {
            return None;
        };
        // Marker timings and words describe only the original audio.
        if original.sha256 == voice.sha256 {
            voice.ms = ms;
            voice.levels = VoiceMeta {
                levels: levels.to_string(),
                ..Default::default()
            }
            .bars();
            voice.transcript = transcript.trim().to_string();
        }
        Some(voice)
    }

    pub(crate) fn entry_dto(&self, e: &Entry) -> TimelineEntry {
        let kind = match &e.body {
            EntryBody::Message { text, attaches, .. } => EntryKind::Message {
                text: text.clone(),
                card: attaches
                    .as_deref()
                    .filter(|item| self.voice_of(&e.space, item).is_none())
                    .and_then(|item| self.card_in_space(&e.space, item)),
            },
            EntryBody::ItemEdited {
                item,
                version,
                note,
                is_undo,
            } => EntryKind::ItemEdited {
                item_id: item.clone(),
                title: self
                    .space_projections
                    .get(&e.space)
                    .and_then(|scoped| scoped.items.get(item))
                    .map(|i| i.current().content.title())
                    .unwrap_or_default(),
                version: *version,
                note: note.clone(),
                is_undo: *is_undo,
            },
            EntryBody::Request { request } => {
                let r = self
                    .space_projections
                    .get(&e.space)
                    .and_then(|scoped| scoped.requests.get(request));
                EntryKind::Request {
                    request_id: request.clone(),
                    title: r.map(|r| r.req.title.clone()).unwrap_or_default(),
                    approved: r.and_then(|r| match r.status {
                        ReqStatus::Pending => None,
                        ReqStatus::Approved => Some(true),
                        ReqStatus::Denied => Some(false),
                    }),
                }
            }
            EntryBody::RequestResolved { request, approved } => {
                let title = self
                    .space_projections
                    .get(&e.space)
                    .and_then(|scoped| scoped.requests.get(request))
                    .map(|r| r.req.title.clone())
                    .unwrap_or_default();
                EntryKind::System {
                    text: format!(
                        "{} {}",
                        if *approved {
                            ts("Aprovado:", "Approved:")
                        } else {
                            ts("Recusado:", "Declined:")
                        },
                        title
                    ),
                }
            }
            EntryBody::System { text } => EntryKind::System { text: text.clone() },
            EntryBody::Background { spec } => EntryKind::Background {
                background: spec.into(),
            },
        };
        let (reply_to, in_thread) = self.reply_parts(e);
        let marks = match &e.body {
            EntryBody::Message { marks, .. } => marks.clone(),
            _ => MessageMarks::default(),
        };
        let me = self.me.clone().unwrap_or_default();
        let mut reactions: Vec<ReactionDto> = Vec::new();
        for (who, emoji) in &marks.reactions {
            match reactions.iter_mut().find(|r| &r.emoji == emoji) {
                Some(r) => r.people.push(self.persona(who)),
                None => reactions.push(ReactionDto {
                    emoji: emoji.clone(),
                    people: vec![self.persona(who)],
                    mine: false,
                }),
            }
        }
        for r in &mut reactions {
            r.mine = r.people.iter().any(|p| p.id == me);
        }
        TimelineEntry {
            text_revision: matches!(&e.body, EntryBody::Message { marks, .. } if !marks.deleted)
                .then(|| {
                    marks
                        .text_revision
                        .clone()
                        .unwrap_or_else(|| e.id().to_string())
                }),
            media: (!marks.deleted)
                .then(|| marks.media.as_ref().map(MessageMediaDto::from))
                .flatten(),
            edited: marks.edited,
            deleted: marks.deleted,
            forwarded: marks.forwarded,
            reactions,
            id: e.id().to_string(),
            seq: e.seq,
            author: self.persona(&e.author),
            at_ms: e.at_ms,
            kind,
            delivery: self.delivery(e),
            voice: self.entry_voice(e),
            reply_to,
            in_thread,
            thread_replies: 0,
        }
    }

    pub(super) fn preview(&self, s: &SpaceState) -> (String, Option<Persona>, Option<Delivery>) {
        for e in s.entries.iter().rev() {
            let text = match &e.body {
                EntryBody::Message { marks, .. } if marks.deleted => {
                    t("Mensagem apagada", "Message deleted")
                }
                EntryBody::Message {
                    attaches: Some(item),
                    ..
                } if self.voice_of(&e.space, item).is_some() => {
                    t("Mensagem de voz", "Voice message")
                }
                EntryBody::Message { text, attaches, .. } => {
                    match attaches.as_deref().and_then(|i| self.card(i)) {
                        Some(c) if text.is_empty() => format!("{} · {}", c.kind_label, c.title),
                        _ => text.clone(),
                    }
                }
                EntryBody::ItemEdited { note, .. } => note.clone(),
                EntryBody::Request { request } => {
                    if let Some(r) = self.state.requests.get(request) {
                        // A request's title already describes the action in natural language.
                        return (r.req.title.clone(), None, None);
                    }
                    continue;
                }
                EntryBody::RequestResolved { .. } | EntryBody::System { .. } => continue,
                EntryBody::Background { spec } => {
                    if spec.style == "photo" || spec.style.starts_with("builtin:") {
                        t(
                            "Definiu uma foto como fundo",
                            "Set a photo as the background",
                        )
                    } else {
                        t("Mudou o fundo da conversa", "Changed the chat background")
                    }
                }
            };
            let delivery = if matches!(e.body, EntryBody::Message { .. })
                && self.me.as_deref() == Some(e.author.as_str())
            {
                Some(self.delivery(e))
            } else {
                None
            };
            return (text, Some(self.persona(&e.author)), delivery);
        }
        (t("Nenhuma mensagem ainda", "No messages yet"), None, None)
    }
}
