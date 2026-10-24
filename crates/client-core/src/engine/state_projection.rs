//! Fold authenticated accepted events into the existing State.
use super::{
    t, BeforeEvent, Entry, EntryBody, Event, EventBody, GrantState, HashMap, Identity, IdentityId,
    ItemContent, ItemKind, MessageMarks, PlanOrigin, Privacy, Projection, ReqStatus, RequestState,
    Role, SpaceKind, SpaceState, State, Version,
};

impl State {
    pub(crate) fn apply_checked(
        &mut self,
        e: &Event,
        identities: &HashMap<IdentityId, Identity>,
        context: &BeforeEvent,
    ) -> Projection {
        if !self.call_transport_valid(e) {
            return Projection::Refused;
        }
        if !self.call_event_valid(e, identities) {
            return Projection::Refused;
        }
        if !self.media_message_valid(&e.space, &e.author, &e.body) {
            return Projection::Refused;
        }
        match self.approval_event_valid(e, identities, context) {
            Projection::Refused => return Projection::Refused,
            Projection::Waiting => {
                if context.ordered() {
                    self.waiting_spaces.insert(e.space.clone());
                }
                return Projection::Waiting;
            }
            Projection::Applied => {}
        }
        let entry = |body| Entry {
            hash: e.hash.clone(),
            client_id: e.client_id.clone(),
            space: e.space.clone(),
            seq: e.seq,
            author: e.author.clone(),
            at_ms: e.at_ms,
            body,
        };
        match &e.body {
            EventBody::SpaceCreated {
                title,
                kind,
                privacy,
            } => {
                self.spaces.insert(
                    e.space.clone(),
                    SpaceState {
                        calls: Vec::new(),
                        call_transport: Vec::new(),
                        id: e.space.clone(),
                        title: title.clone(),
                        kind: *kind,
                        privacy: *privacy,
                        members: vec![(e.author.clone(), Role::Owner)],
                        entries: Vec::new(),
                        last_at_ms: e.at_ms,
                        integrity_error: None,
                    },
                );
            }
            EventBody::MemberAdded { identity, role } => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    let changing = !s
                        .members
                        .iter()
                        .any(|(id, existing)| id == identity && existing == role);
                    s.members.retain(|(id, _)| id != identity);
                    s.members.push((identity.clone(), *role));
                    if changing {
                        s.call_transport.clear();
                        for call in &mut s.calls {
                            call.invalidated = true;
                        }
                    }
                }
            }
            EventBody::MemberRemoved { identity } => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.members.retain(|(id, _)| id != identity);
                    s.call_transport.clear();
                    for call in &mut s.calls {
                        call.invalidated = true;
                    }
                }
            }
            EventBody::SpaceEncrypted => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.privacy = Privacy::EndToEnd;
                    s.entries.push(entry(EntryBody::System {
                        text: t(
                            "Criptografia de ponta a ponta ativada. Só os membros leem as mensagens novas.",
                            "End-to-end encryption is on. Only members can read new messages.",
                        ),
                    }));
                }
            }
            EventBody::MessagePosted {
                text,
                attaches,
                reply,
                forwarded,
                media,
                ..
            } => {
                // Older Android builds posted the file, then a marker naming its Item.
                let legacy_voice = text
                    .strip_prefix("⟦voice:")
                    .and_then(|body| body.split_once(':'))
                    .map(|(item, _)| item);
                let voice_item = attaches.as_deref().or(legacy_voice);
                let voice_attached = !forwarded
                    && ((text.is_empty() && reply.is_some()) || legacy_voice.is_some())
                    && self.writer(&e.space, &e.author)
                    && voice_item.is_some_and(|i| {
                        self.items.get(i).is_some_and(|it| {
                            it.space == e.space
                                && it.versions.first().is_some_and(|v| v.author == e.author)
                                && matches!(&it.current().content, ItemContent::File(d) if d.is_voice())
                        })
                    });
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    // Only the author's reply/legacy marker replaces the implicit file
                    // message. Forwarding must leave the original entry in place.
                    if let Some(item) = voice_item.filter(|_| voice_attached) {
                        if let Some(at) = s.entries.iter().rposition(|x| {
                            x.author == e.author
                                && matches!(&x.body, EntryBody::Message { text, attaches: Some(a), reply: None, marks } if a == item && text.is_empty() && !marks.forwarded)
                        }) {
                            s.entries.remove(at);
                        }
                    }
                    s.entries.push(entry(EntryBody::Message {
                        text: text.clone(),
                        attaches: attaches.clone(),
                        reply: reply.clone(),
                        marks: MessageMarks {
                            forwarded: *forwarded,
                            media: media.clone(),
                            ..Default::default()
                        },
                    }));
                    s.last_at_ms = s.last_at_ms.max(e.at_ms);
                }
                if let Some(metadata) = media.as_ref() {
                    self.media_groups
                        .record(&e.space, &e.author, reply, *forwarded, metadata);
                }
            }
            EventBody::ChatPrefsSet { space, prefs } => {
                // Only my own Personal Space carries prefs; anywhere else they mean nothing.
                let mine = self.spaces.get(&e.space).is_some_and(|s| {
                    s.kind == SpaceKind::Personal && s.members.iter().any(|(m, _)| *m == e.author)
                });
                if mine {
                    self.chat_prefs.insert(space.clone(), prefs.clone());
                }
            }
            EventBody::MessageAnnotated { target, note } => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    let found = s.entries.iter_mut().rev().find(|x| x.id() == target);
                    if let Some(Entry {
                        author,
                        body:
                            EntryBody::Message {
                                text,
                                attaches,
                                marks,
                                ..
                            },
                        ..
                    }) = found
                    {
                        if let Some(new_text) = marks.apply(author, &e.author, note) {
                            *text = new_text;
                            if marks.deleted {
                                *attaches = None;
                            }
                        }
                    }
                }
            }
            EventBody::BackgroundSet { background } => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.entries.push(entry(EntryBody::Background {
                        spec: background.clone(),
                    }));
                    s.last_at_ms = s.last_at_ms.max(e.at_ms);
                }
            }
            EventBody::ItemCreated {
                item,
                kind,
                content,
                origin,
            } => {
                self.create_item(e, item, *kind, content, origin);
            }
            EventBody::ItemVersioned {
                item,
                content,
                note,
            } => {
                if let Some(it) = self.items.get_mut(item) {
                    let number = it.versions.len() as u32 + 1;
                    let content =
                        crate::apps::reconcile_concurrent(&it.current().content, content, e.at_ms)
                            .unwrap_or_else(|| content.clone());
                    it.has_voice_history |=
                        matches!(&content, ItemContent::File(file) if file.is_voice());
                    it.versions.push(Version {
                        number,
                        content,
                        author: e.author.clone(),
                        at_ms: e.at_ms,
                        note: note.clone(),
                        is_undo: false,
                    });
                    if let Some(s) = self.spaces.get_mut(&e.space) {
                        s.entries.push(entry(EntryBody::ItemEdited {
                            item: item.clone(),
                            version: number,
                            note: note.clone(),
                            is_undo: false,
                        }));
                        s.last_at_ms = s.last_at_ms.max(e.at_ms);
                    }
                }
            }
            EventBody::ItemReverted {
                item,
                to_version,
                note,
            } => {
                if let Some(it) = self.items.get_mut(item) {
                    if let Some(target) = it
                        .versions
                        .iter()
                        .find(|v| v.number == *to_version)
                        .cloned()
                    {
                        let number = it.versions.len() as u32 + 1;
                        it.has_voice_history |=
                            matches!(&target.content, ItemContent::File(file) if file.is_voice());
                        it.versions.push(Version {
                            number,
                            content: target.content,
                            author: e.author.clone(),
                            at_ms: e.at_ms,
                            note: note.clone(),
                            is_undo: true,
                        });
                        if let Some(s) = self.spaces.get_mut(&e.space) {
                            s.entries.push(entry(EntryBody::ItemEdited {
                                item: item.clone(),
                                version: number,
                                note: note.clone(),
                                is_undo: true,
                            }));
                        }
                    }
                }
            }
            EventBody::GrantIssued { grant } => {
                self.grants.push(GrantState {
                    grant: grant.clone(),
                    at_ms: e.at_ms,
                    revoked: false,
                });
            }
            EventBody::GrantRevoked { grant } => {
                for g in self
                    .grants
                    .iter_mut()
                    .filter(|g| &g.grant.id == grant && g.grant.grantor == e.author)
                {
                    g.revoked = true;
                }
            }
            EventBody::RequestOpened { request } => {
                if let Some(p) = &request.proposal {
                    if p.ai_cost_cents > 0 {
                        self.usage
                            .push((request.agent.clone(), p.ai_cost_cents, e.at_ms));
                    }
                }
                self.request_order.push(request.id.clone());
                self.requests.insert(
                    request.id.clone(),
                    RequestState {
                        req: request.clone(),
                        space: e.space.clone(),
                        opened_ms: e.at_ms,
                        opened_content_hash: event_log::content_hash_of(e),
                        resolved_content_hash: None,
                        status: ReqStatus::Pending,
                        resolved_ms: None,
                        resolution: None,
                        review: None,
                    },
                );
                self.tool_review_successor_opened(request);
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.entries.push(entry(EntryBody::Request {
                        request: request.id.clone(),
                    }));
                }
            }
            EventBody::ToolReviewRequested { request, .. } => {
                if let Some(r) = self.requests.get_mut(request) {
                    r.review = Some(
                        crate::agent_binary_documents::tool_approvals::renewal::ReviewIntent {
                            id: e.client_id.clone(),
                            at_ms: e.at_ms,
                            successor: None,
                        },
                    );
                }
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.entries.push(entry(EntryBody::System {
                        text: t(
                            "Nova revisão solicitada. A aprovação anterior não foi renovada.",
                            "New review requested. The previous approval was not renewed.",
                        ),
                    }));
                }
            }
            EventBody::RequestResolved {
                request,
                approved,
                resolution,
                ..
            } => {
                let original = self.requests[request].clone();
                if let Some(receipt) = resolution {
                    self.apply_resolution(e, &original, *approved, receipt);
                }
                self.resolved_order.push(request.clone());
                if let Some(r) = self.requests.get_mut(request) {
                    r.status = if *approved {
                        ReqStatus::Approved
                    } else {
                        ReqStatus::Denied
                    };
                    r.resolved_ms = Some(e.at_ms);
                    r.resolution = resolution.clone();
                    r.resolved_content_hash = Some(event_log::content_hash_of(e));
                }
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.entries.push(entry(EntryBody::RequestResolved {
                        request: request.clone(),
                        approved: *approved,
                    }));
                }
            }
            EventBody::CallTransportV1 { .. } => self.apply_call_transport(e),
            EventBody::CallV1 { .. } => {
                self.apply_call(e);
            }
            EventBody::UsageRecorded { agent, cents, .. } => {
                self.usage.push((agent.clone(), *cents, e.at_ms));
            }
            EventBody::OwnerPolicyOptionsRequested { .. }
            | EventBody::OwnerPolicyOptionsReported { .. }
            | EventBody::OwnerPolicyOptionsRefused { .. }
            | EventBody::OwnerPolicySubmitted { .. }
            | EventBody::OwnerPolicyReported { .. }
            | EventBody::ProfileKeyShared { .. }
            | EventBody::Checkpoint { .. }
            | EventBody::DeviceJoining { .. }
            | EventBody::Sealed { .. }
            | EventBody::Unsupported { .. } => {}
        }
        match &e.body {
            EventBody::ItemCreated {
                item,
                kind: ItemKind::Plan,
                content: ItemContent::Plan(_),
                ..
            } => {
                self.plan_origins
                    .insert(item.clone(), context.creation_origin(e, item));
            }
            EventBody::RequestResolved {
                request,
                approved: true,
                ..
            } => {
                if let Some(proposal) = self.requests[request].req.proposal.as_ref() {
                    if proposal.kind == ItemKind::Plan {
                        self.plan_origins
                            .insert(proposal.item.clone(), PlanOrigin::Proposal);
                    }
                }
            }
            _ => {}
        }
        Projection::Applied
    }
}
