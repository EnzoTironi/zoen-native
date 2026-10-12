use std::collections::{HashMap, HashSet};

use roda_log::{content_hash, content_hash_of, SpaceLog};
use roda_store::Store;
use roda_types::*;

use crate::engine::{State, R};
use crate::CoreError;

use super::{confirmed::Prefix, invalid, item_event, source, PayloadRead};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Projection {
    Applied,
    Refused,
    Waiting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Currentness {
    Current,
    Changed,
    Waiting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlanOrigin {
    Explicit(String),
    Proposal,
    Pending,
}

#[derive(Clone)]
pub(crate) enum PlanRead {
    Ready { document: PlanDoc, legacy: bool },
    Changed,
    Waiting,
}

#[derive(Clone, Copy)]
enum Mode {
    Confirmed,
    Local,
    Pending,
}

/// Only this module can construct the event-bound read. Its document is derived
/// from a verified log prefix and locally authenticated, bounded payload bytes.
pub(crate) struct BeforeEvent {
    event: String,
    mode: Mode,
    plan: PlanRead,
}

fn binding(event: &Event) -> String {
    content_hash(&(
        content_hash_of(event),
        &event.space,
        &event.author,
        &event.client_id,
        &event.body,
        event.at_ms,
        &event.seen,
        &event.device,
        &event.cert,
        &event.sig,
    ))
}

impl BeforeEvent {
    pub(crate) fn matches(&self, event: &Event) -> bool {
        self.event == binding(event)
    }

    pub(crate) fn plan(&self) -> &PlanRead {
        &self.plan
    }

    pub(crate) fn ordered(&self) -> bool {
        !matches!(self.mode, Mode::Pending)
    }

    pub(crate) fn creation_origin(&self, event: &Event, item: &str) -> PlanOrigin {
        match self.mode {
            Mode::Confirmed | Mode::Local => {
                PlanOrigin::Explicit(source(&event.space, item, event, true).event_id)
            }
            Mode::Pending => PlanOrigin::Pending,
        }
    }
}

/// One replay owns its payload snapshot. Repeated decisions reuse a fold until
/// that Item's ordered history changes; no page-session or network reader enters it.
pub(crate) struct PlanReplay<'a> {
    store: &'a Store,
    logs: &'a HashMap<SpaceId, SpaceLog>,
    origins: HashMap<ItemId, HashSet<SpaceId>>,
    positions: HashMap<(SpaceId, ItemId), Vec<usize>>,
    cache: HashMap<(SpaceId, ItemId), (Option<usize>, String, PlanRead)>,
    payloads: HashMap<String, PayloadRead>,
    pending_cutovers: Option<HashSet<ItemId>>,
    indexed: bool,
}

impl<'a> PlanReplay<'a> {
    pub(crate) fn new(store: &'a Store, logs: &'a HashMap<SpaceId, SpaceLog>) -> Self {
        Self {
            store,
            logs,
            origins: HashMap::new(),
            positions: HashMap::new(),
            cache: HashMap::new(),
            payloads: HashMap::new(),
            pending_cutovers: None,
            indexed: false,
        }
    }

    fn index(&mut self) {
        if self.indexed {
            return;
        }
        let mut origins: HashMap<ItemId, HashSet<SpaceId>> = HashMap::new();
        let mut positions: HashMap<(SpaceId, ItemId), Vec<usize>> = HashMap::new();
        for (space, log) in self.logs {
            for (index, event) in log.events().iter().enumerate() {
                if let EventBody::ItemCreated { item, .. } = &event.body {
                    origins
                        .entry(item.clone())
                        .or_default()
                        .insert(space.clone());
                }
                let item = match &event.body {
                    EventBody::ItemCreated { item, .. }
                    | EventBody::ItemVersioned { item, .. }
                    | EventBody::ItemReverted { item, .. } => Some(item),
                    _ => None,
                };
                if let Some(item) = item {
                    positions
                        .entry((space.clone(), item.clone()))
                        .or_default()
                        .push(index);
                }
            }
        }
        self.origins = origins;
        self.positions = positions;
        self.indexed = true;
    }

    fn pending_cutover(&mut self, item: &str) -> R<bool> {
        if self.pending_cutovers.is_none() {
            self.pending_cutovers = Some(
                self.store
                    .outbox()?
                    .into_iter()
                    .filter_map(|pending| {
                        if pending.failed {
                            return None;
                        }
                        match pending.event.body {
                            EventBody::ItemVersioned {
                                item,
                                content:
                                    ItemContent::Page(PageContent {
                                        manifest: Some(PageManifest::PlanV1(_)),
                                        ..
                                    }),
                                ..
                            } => Some(item),
                            _ => None,
                        }
                    })
                    .collect(),
            );
        }
        Ok(self
            .pending_cutovers
            .as_ref()
            .is_some_and(|items| items.contains(item)))
    }

    pub(crate) fn before(
        &mut self,
        state: &State,
        identities: &HashMap<IdentityId, Identity>,
        event: &Event,
        end: usize,
    ) -> R<BeforeEvent> {
        let actual = self
            .logs
            .get(&event.space)
            .and_then(|log| log.events().get(end));
        if actual.is_none_or(|actual| actual != event) {
            return Err(invalid(
                "decision is outside the authenticated Space frontier",
            ));
        }
        self.context(state, identities, event, end, Mode::Confirmed)
    }

    pub(crate) fn local(
        &mut self,
        state: &State,
        identities: &HashMap<IdentityId, Identity>,
        event: &Event,
    ) -> R<BeforeEvent> {
        let end = self.logs.get(&event.space).map_or(0, SpaceLog::len);
        self.context(state, identities, event, end, Mode::Local)
    }

    pub(crate) fn pending(
        &mut self,
        state: &State,
        identities: &HashMap<IdentityId, Identity>,
        event: &Event,
    ) -> R<BeforeEvent> {
        let end = self.logs.get(&event.space).map_or(0, SpaceLog::len);
        self.context(state, identities, event, end, Mode::Pending)
    }

    fn context(
        &mut self,
        state: &State,
        identities: &HashMap<IdentityId, Identity>,
        event: &Event,
        end: usize,
        mode: Mode,
    ) -> R<BeforeEvent> {
        let plan = if state.waiting_spaces.contains(&event.space)
            || !state.approval_event_guards(event, identities)
        {
            PlanRead::Changed
        } else if let EventBody::RequestResolved {
            request,
            approved: true,
            ..
        } = &event.body
        {
            match state
                .requests
                .get(request)
                .and_then(|request| request.req.item.as_deref())
            {
                Some(item) => match self.read(state, &event.space, item, end, mode)? {
                    PlanRead::Ready { legacy: false, .. } if matches!(mode, Mode::Pending) => {
                        PlanRead::Changed
                    }
                    read => read,
                },
                None => PlanRead::Changed,
            }
        } else {
            PlanRead::Changed
        };
        Ok(BeforeEvent {
            event: binding(event),
            mode,
            plan,
        })
    }

    pub(crate) fn current(
        &mut self,
        state: &State,
        request: &AgentRequest,
        space: &str,
    ) -> R<Currentness> {
        if state.waiting_spaces.contains(space) {
            return Ok(Currentness::Waiting);
        }
        if !state.writer(space, &request.agent) {
            return Ok(Currentness::Changed);
        }
        let plan = match request.item.as_deref() {
            Some(item) => {
                let end = self.logs.get(space).map_or(0, SpaceLog::len);
                self.read(state, space, item, end, Mode::Pending)?
            }
            None => PlanRead::Changed,
        };
        Ok(state.request_currentness(request, space, &plan))
    }

    fn read(
        &mut self,
        state: &State,
        space: &str,
        item: &str,
        end: usize,
        mode: Mode,
    ) -> R<PlanRead> {
        let Some(projected) = state
            .items
            .get(item)
            .filter(|item| item.space == space && item.kind == ItemKind::Plan)
        else {
            return Ok(PlanRead::Changed);
        };
        self.index();
        let pending_cutover = matches!(mode, Mode::Pending) && self.pending_cutover(item)?;
        if self
            .origins
            .get(item)
            .is_some_and(|spaces| spaces.iter().any(|origin| origin != space))
        {
            return Ok(PlanRead::Changed);
        }
        let origin = state.plan_origins.get(item);
        let log = self.logs.get(space);
        let prefix = Prefix::from_log(space, log, end)?;
        let Some(PlanOrigin::Explicit(origin)) = origin else {
            let proposal = matches!(origin, Some(PlanOrigin::Proposal));
            let pending =
                matches!(origin, Some(PlanOrigin::Pending)) && matches!(mode, Mode::Pending);
            let canonical = log.is_some_and(|log| {
                log.events()[..end].iter().any(|event| {
                    item_event(event, item)
                        && matches!(
                            event.body,
                            EventBody::ItemVersioned {
                                content: ItemContent::Page(_),
                                ..
                            }
                        )
                })
            });
            return Ok(match &projected.current().content {
                ItemContent::Plan(document)
                    if (proposal || pending)
                        && !canonical
                        && (!matches!(mode, Mode::Pending) || !pending_cutover) =>
                {
                    PlanRead::Ready {
                        document: document.clone(),
                        legacy: true,
                    }
                }
                _ => PlanRead::Changed,
            });
        };
        let key = (space.to_owned(), item.to_owned());
        let last = self.positions.get(&key).and_then(|positions| {
            positions
                .get(
                    positions
                        .partition_point(|index| *index < end)
                        .checked_sub(1)?,
                )
                .copied()
        });
        let cached = self
            .cache
            .get(&key)
            .filter(|(position, known_origin, _)| *position == last && known_origin == origin);
        let read = if let Some((_, _, read)) = cached {
            read.clone()
        } else {
            let folded = prefix.fold(item, |page| {
                let key = content_hash(page);
                if let Some(payload) = self.payloads.get(&key) {
                    return Ok(payload.clone());
                }
                let payload = super::read_plan_payload(self.store, page)?;
                self.payloads.insert(key, payload.clone());
                Ok(payload)
            });
            let read = match folded {
                Err(CoreError::Invalid { .. }) => PlanRead::Changed,
                Err(error) => return Err(error),
                Ok((fold, _))
                    if fold
                        .origin_tag()
                        .is_none_or(|(_, _, creation)| creation != origin) =>
                {
                    PlanRead::Changed
                }
                Ok((fold, _)) if fold.waiting => PlanRead::Waiting,
                Ok((fold, _)) => match fold.document {
                    Some(document) => match document.view() {
                        Ok(view) => PlanRead::Ready {
                            document: view.to_plan_doc(),
                            legacy: fold.frozen_at.is_none(),
                        },
                        Err(_) => PlanRead::Changed,
                    },
                    None => PlanRead::Changed,
                },
            };
            self.cache.insert(key, (last, origin.clone(), read.clone()));
            read
        };
        if matches!(mode, Mode::Pending) {
            return Ok(match read {
                PlanRead::Ready { legacy: true, .. } if !pending_cutover => {
                    match &projected.current().content {
                        ItemContent::Plan(document) => PlanRead::Ready {
                            document: document.clone(),
                            legacy: true,
                        },
                        _ => PlanRead::Changed,
                    }
                }
                other => other,
            });
        }
        Ok(read)
    }
}
