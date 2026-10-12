use std::collections::BTreeMap;

use roda_docs::plan::PlanUpdate;
use roda_log::SpaceLog;
use roda_types::{Event, EventBody, ItemContent, ItemKind, PageContent, Role};

use crate::engine::R;

use super::{
    event_content, invalid, item_event, legacy_intent, source, Acceptance, PayloadRead, PlanFold,
    Slot, Tape,
};

pub(super) struct Prefix<'a> {
    space: &'a str,
    events: &'a [Event],
}

impl<'a> Prefix<'a> {
    pub(super) fn from_log(space: &'a str, log: Option<&'a SpaceLog>, end: usize) -> R<Self> {
        let events = match log {
            Some(log) if log.space() == space => log.events().get(..end),
            None if end == 0 => Some(&[][..]),
            _ => None,
        }
        .ok_or_else(|| invalid("Plan prefix is outside the authenticated Space history"))?;
        Ok(Self { space, events })
    }

    pub(super) fn fold(
        self,
        item: &str,
        mut read_payload: impl FnMut(&PageContent) -> R<PayloadRead>,
    ) -> R<(PlanFold, u32)> {
        let mut tape = Tape::new(self.space, item);
        let mut fold = PlanFold {
            slots: Vec::new(),
            document: None,
            frozen_at: None,
            accepted_current: None,
            waiting: false,
            pending_legacy: false,
            manifest: None,
            prefixes: Vec::new(),
        };
        let mut roster = BTreeMap::new();
        let mut creation_seen = false;
        let mut version_number = 0_u32;
        for event in self.events {
            match &event.body {
                EventBody::SpaceCreated { .. } => {
                    roster.insert(event.author.clone(), Role::Owner);
                }
                EventBody::MemberAdded { identity, role } => {
                    roster.insert(identity.clone(), *role);
                }
                EventBody::MemberRemoved { identity } => {
                    roster.remove(identity);
                }
                _ => {}
            }
            if !item_event(event, item) {
                continue;
            }
            let source = source(self.space, item, event, true);
            let (content, reverted_to) = event_content(event, &fold.slots);
            // Match legacy version addresses: an unresolved undo never created a
            // Version in ItemState. Refused versions still have a read-only slot.
            let created = matches!(event.body, EventBody::ItemCreated { .. });
            let duplicate = created && creation_seen;
            let has_number = !duplicate
                && (created || creation_seen && (reverted_to.is_none() || content.is_some()));
            let number = if has_number {
                version_number = version_number
                    .checked_add(1)
                    .ok_or_else(|| invalid("Plan history is too large; nothing was discarded"))?;
                Some(version_number)
            } else {
                None
            };
            let writer = roster
                .get(&event.author)
                .is_some_and(|role| *role != Role::Reader);
            creation_seen |= created;
            let acceptance = if fold.waiting {
                Acceptance::Waiting("an earlier Plan payload is still arriving".into())
            } else if !writer {
                Acceptance::Refused("only an active writing member can change a Plan".into())
            } else if duplicate {
                Acceptance::Refused("the Plan's creation identity is immutable".into())
            } else if created
                && !matches!(
                    event.body,
                    EventBody::ItemCreated {
                        kind: ItemKind::Plan,
                        content: ItemContent::Plan(_),
                        ..
                    }
                )
            {
                Acceptance::Refused("the authenticated Plan creation is incompatible".into())
            } else if !created && tape.origin.is_none() {
                Acceptance::Refused("the Plan has no authenticated legacy creation".into())
            } else if fold.frozen_at.is_some() && legacy_intent(event) {
                Acceptance::Refused(
                    "incompatible legacy write after Plan cutover; history retained".into(),
                )
            } else {
                match &content {
                    Some(ItemContent::Plan(document)) => {
                        tape.push(event, &source, document)?;
                        fold.document = None;
                        fold.accepted_current = number;
                        Acceptance::Legacy
                    }
                    Some(ItemContent::Page(page)) if reverted_to.is_none() => {
                        match tape.validate_manifest(page) {
                            Err(error) => Acceptance::Refused(error.to_string()),
                            Ok(()) => match read_payload(page)? {
                                PayloadRead::Waiting(reason) => {
                                    fold.waiting = true;
                                    Acceptance::Waiting(reason)
                                }
                                PayloadRead::Malformed(reason) => Acceptance::Refused(reason),
                                PayloadRead::Ready(bytes) => match PlanUpdate::from_json(&bytes) {
                                    Err(error) => Acceptance::Refused(error.to_string()),
                                    Ok(update) => {
                                        if fold.document.is_none() {
                                            fold.document = Some(tape.document()?);
                                        }
                                        let document =
                                            fold.document.as_mut().expect("initialized above");
                                        match document.import(&update) {
                                            Err(error) => Acceptance::Refused(error.to_string()),
                                            Ok(()) => {
                                                if fold.frozen_at.is_none() {
                                                    fold.frozen_at = Some(source.clone());
                                                }
                                                fold.accepted_current = number;
                                                Acceptance::Canonical(document.version())
                                            }
                                        }
                                    }
                                },
                            },
                        }
                    }
                    _ => Acceptance::Refused("this version is not supported Plan content".into()),
                }
            };
            fold.slots.push(Slot {
                number,
                source,
                content,
                reverted_to,
                acceptance,
            });
        }
        if fold.document.is_none() && !tape.history.is_empty() {
            fold.document = Some(tape.document()?);
        }
        fold.manifest = tape.prefixes.last().cloned();
        fold.prefixes = tape.prefixes;
        Ok((fold, version_number))
    }
}
