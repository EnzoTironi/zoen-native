//! Persistence, identity signing and projection reload.
use super::*;

impl Engine {
    pub(crate) fn merge_space_projections(
        projections: &HashMap<SpaceId, State>,
        order: &[SpaceId],
    ) -> State {
        let mut item_spaces = HashMap::<&str, usize>::new();
        let mut request_spaces = HashMap::<&str, usize>::new();
        for scoped in projections.values() {
            for item in scoped.items.keys() {
                *item_spaces.entry(item).or_default() += 1;
            }
            for request in scoped.requests.keys() {
                *request_spaces.entry(request).or_default() += 1;
            }
        }
        let mut state = State::default();
        for space in order {
            let Some(scoped) = projections.get(space) else {
                continue;
            };
            if let Some(projected) = scoped.spaces.get(space) {
                state.spaces.insert(space.clone(), projected.clone());
            }
            state.media_groups.extend(&scoped.media_groups);
            state
                .recovery_items
                .extend(scoped.recovery_items.iter().cloned());
            // Only a Personal Space projects prefs (checked when applied).
            state.chat_prefs.extend(
                scoped
                    .chat_prefs
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
            for (item, projected) in &scoped.items {
                if item_spaces.get(item.as_str()) == Some(&1) {
                    state.items.insert(item.clone(), projected.clone());
                    if let Some(origin) = scoped.plan_origins.get(item) {
                        state.plan_origins.insert(item.clone(), origin.clone());
                    }
                }
            }
            for (request, projected) in &scoped.requests {
                if request_spaces.get(request.as_str()) == Some(&1) {
                    state.requests.insert(request.clone(), projected.clone());
                }
            }
            state.request_order.extend(
                scoped
                    .request_order
                    .iter()
                    .filter(|request| request_spaces.get(request.as_str()) == Some(&1))
                    .cloned(),
            );
            state.resolved_order.extend(
                scoped
                    .resolved_order
                    .iter()
                    .filter(|request| request_spaces.get(request.as_str()) == Some(&1))
                    .cloned(),
            );
            state.grants.extend(scoped.grants.iter().cloned());
            state.usage.extend(scoped.usage.iter().cloned());
            state
                .waiting_spaces
                .extend(scoped.waiting_spaces.iter().cloned());
        }
        state
    }

    pub fn open(path: &str) -> R<Self> {
        let store = if path == ":memory:" {
            Store::in_memory()?
        } else {
            Store::open(path)?
        };
        Self::from_store(store, path)
    }

    pub(crate) fn from_store(store: Store, path: &str) -> R<Self> {
        let mut engine = Engine {
            store,
            db_path: path.to_string(),
            logs: HashMap::new(),
            broken_spaces: HashMap::new(),
            approval_replay_needed: false,
            space_order: Vec::new(),
            identities: HashMap::new(),
            identity_order: Vec::new(),
            signers: HashMap::new(),
            me: None,
            state: State::default(),
            space_projections: HashMap::new(),
            policy: Policy::default(),
            index_dirty: true,
            net: Default::default(),
            pages: Default::default(),
        };
        engine.migrate_event_format()?;
        engine.migrate_mls()?;
        engine.reload()?;
        Ok(engine)
    }

    /// Recarrega tudo do disco: reverifica cada log e reprojeta do zero.
    pub fn reload(&mut self) -> R<()> {
        self.pages.epoch = std::sync::Arc::new(());
        self.pages
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        self.logs.clear();
        self.space_order.clear();
        self.identities.clear();
        self.identity_order.clear();
        self.signers.clear();
        self.broken_spaces.clear();
        self.index_dirty = true;
        self.me = self.store.meta("me")?;
        for (identity, secret) in self.store.identities()? {
            if secret.is_none()
                && self.me.as_deref() != Some(identity.id.as_str())
                && !event_log::agent_owner::profile_authorized(&identity)
            {
                continue;
            }
            if let Some(secret) = secret {
                self.signers
                    .insert(identity.id.clone(), Signer::from_secret(&secret));
            }
            self.identity_order.push(identity.id.clone());
            self.identities.insert(identity.id.clone(), identity);
        }
        for space in self.store.space_ids()? {
            let events = self.store.events(&space)?;
            match SpaceLog::from_events(space.clone(), events.clone()) {
                Ok(log) => {
                    self.logs.insert(space.clone(), log);
                }
                Err(err) => {
                    // Log adulterado: não projetamos nada dele, mas o Espaço aparece com o erro.
                    let title = events
                        .iter()
                        .find_map(|e| match &e.body {
                            EventBody::SpaceCreated { title, .. } => Some(title.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| t("Espaço", "Space"));
                    self.broken_spaces.insert(
                        space.clone(),
                        SpaceState {
                            calls: Vec::new(),
                            call_transport: Vec::new(),
                            id: space.clone(),
                            title,
                            kind: SpaceKind::Group,
                            privacy: Privacy::EndToEnd,
                            members: vec![],
                            entries: vec![],
                            last_at_ms: 0,
                            integrity_error: Some(err.to_string()),
                        },
                    );
                }
            }
            self.space_order.push(space);
        }
        self.reload_sync()?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.identities.is_empty()
    }

    pub(crate) fn me_id(&self) -> R<IdentityId> {
        self.me.clone().ok_or_else(|| not_found("identidade local"))
    }

    // ── escrita de baixo nível ──

    pub(crate) fn create_identity(&mut self, mut identity: Identity, local: bool) -> R<IdentityId> {
        let signer = Signer::generate();
        identity.id = signer.id();
        if local && identity.kind == IdentityKind::Agent && identity.owner_proof.is_none() {
            identity.owner_proof = self
                .net
                .author
                .as_ref()
                .filter(|owner| identity.owner.as_deref() == Some(&owner.identity))
                .and_then(|owner| event_log::agent_owner::authorize(owner, &identity.id))
                .map(Box::new);
        }
        let secret = signer.secret();
        self.store
            .put_identity(&identity, if local { Some(&secret) } else { None })?;
        if local {
            self.signers.insert(identity.id.clone(), signer);
        }
        self.identity_order.push(identity.id.clone());
        let id = identity.id.clone();
        self.identities.insert(id.clone(), identity);
        Ok(id)
    }

    pub(crate) fn append_at(
        &mut self,
        space: &str,
        author: &str,
        at_ms: i64,
        body: EventBody,
    ) -> R<Event> {
        if self.approval_replay_needed {
            self.reproject_approval_overlay()?;
        }
        if !self.state.media_message_valid(space, author, &body) {
            return Err(CoreError::Invalid {
                reason: t(
                    "metadata de anexo inválida ou não autorizada",
                    "invalid or unauthorized attachment metadata",
                ),
            });
        }
        let signer = self.author_for(author)?;
        if matches!(
            &body,
            EventBody::CallV1 { .. } | EventBody::CallTransportV1 { .. }
        ) {
            let probe = signer.sign_event(space, "call-admission", at_ms, None, body.clone());
            if !self.state.call_event_valid(&probe, &self.identities)
                || !self.state.call_transport_valid(&probe)
            {
                return Err(CoreError::Forbidden {
                    reason: "Invalid or unauthorized call transition".into(),
                });
            }
        }
        if self.net.synced.contains(space) {
            // Only this device's account publishes to the relay for now; local agents
            // join shared chats as real members with the agent runtime (milestone 3).
            if self
                .net
                .account
                .as_ref()
                .is_some_and(|a| a.identity != signer.identity)
            {
                return Err(CoreError::Forbidden {
                    reason: t(
                        "agentes ainda não escrevem em conversas compartilhadas",
                        "agents can't write in shared chats yet",
                    ),
                });
            }
            // Relay-ordered Space: sign now, queue, show it right away; the relay assigns
            // the sequence number when it gets there (now or after being offline).
            return self.append_synced(space, &signer, at_ms, body);
        }
        let is_new = !self.logs.contains_key(space);
        if is_new && !matches!(body, EventBody::SpaceCreated { .. }) {
            return Err(not_found(&t("Espaço", "Space")));
        }
        let client_id = new_ulid(at_ms);
        let seen = self.logs.get(space).and_then(SpaceLog::head);
        let signed = signer.sign_event(space, &client_id, at_ms, seen, body);
        let empty = State::default();
        let scoped = self.space_projections.get(space).unwrap_or(&empty);
        let context =
            PlanReplay::new(&self.store, &self.logs).local(scoped, &self.identities, &signed)?;
        if scoped.approval_event_valid(&signed, &self.identities, &context) != Projection::Applied {
            return Err(CoreError::Invalid {
                reason: t(
                    "pedido, decisão ou uso não autorizado ou aguardando conteúdo",
                    "unauthorized request, decision or usage, or content still arriving",
                ),
            });
        }
        let event = self
            .logs
            .entry(space.to_string())
            .or_insert_with(|| SpaceLog::new(space))
            .sequence(signed)
            .clone();
        if let Err(e) = self.store.append_event(&event) {
            // Mantém memória e disco iguais: desfaz o append em memória recarregando.
            self.reload()?;
            return Err(e.into());
        }
        if is_new {
            self.space_order.push(space.to_string());
        }
        self.space_projections
            .entry(space.to_owned())
            .or_default()
            .apply_checked(&event, &self.identities, &context);
        self.state = Self::merge_space_projections(&self.space_projections, &self.space_order);
        self.index_dirty = true;
        Ok(event)
    }

    pub(crate) fn append(&mut self, space: &str, author: &str, body: EventBody) -> R<Event> {
        self.append_at(space, author, now_ms(), body)
    }

    pub(crate) fn space_state(&self, id: &str) -> R<&SpaceState> {
        self.state
            .spaces
            .get(id)
            .ok_or_else(|| not_found(&t("Espaço", "Space")))
    }

    pub(crate) fn item_state(&self, id: &str) -> R<&ItemState> {
        self.state.items.get(id).ok_or_else(|| not_found("Item"))
    }

    pub(crate) fn is_member(&self, space: &str, who: &str) -> bool {
        self.state
            .spaces
            .get(space)
            .map(|s| s.members.iter().any(|(m, _)| m == who))
            .unwrap_or(false)
    }

    pub fn wipe(&mut self) -> R<()> {
        let tx = self
            .store
            .conn()
            .unchecked_transaction()
            .map_err(|e| CoreError::Storage {
                message: e.to_string(),
            })?;
        self.store.wipe()?;
        self.store.wipe_sync()?;
        self.store.wipe_profiles()?;
        ::mls::clear_recovery_markers(&tx).map_err(crate::mls::mls_err)?;
        // `wipe` empties `meta` too: keep the format marker, or the next open takes this
        // device's fresh v2 logs for old demo data and erases them.
        self.store.set_meta(
            crate::sync::EVENT_FORMAT_META,
            crate::sync::EVENT_FORMAT_VALUE,
        )?;
        tx.commit().map_err(|e| CoreError::Storage {
            message: e.to_string(),
        })?;
        self.reload()
    }

    /// Who signs for `author` here: this device's key for the account, or a local
    /// persona's own key (demo personas and on-device agents).
    pub(crate) fn author_for(&self, author: &str) -> R<Author> {
        if let Some(a) = self.net.account_author(author) {
            return Ok(a);
        }
        self.signers
            .get(author)
            .map(|s| Author::root(s.clone()))
            .ok_or_else(|| CoreError::Forbidden {
                reason: t(
                    "esta Identidade não assina neste aparelho",
                    "this Identity doesn’t sign on this device",
                ),
            })
    }
}
