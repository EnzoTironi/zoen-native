//! Bounded current pin pages. Changed content, visibility or pin state retires a cursor.
use super::{
    changed, Deserialize, Engine, EntryBody, MessagePinPage, MessagePinScope, Serialize, R,
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    revision: String,
    offset: usize,
}
impl Engine {
    pub(crate) fn message_pins(
        &self,
        scope: &MessagePinScope,
        cursor: Option<&str>,
        limit: u32,
    ) -> R<MessagePinPage> {
        self.pin_owner(scope)?;
        let mut entries: Vec<_> = self
            .space_state(&scope.space_id)?
            .entries
            .iter()
            .filter_map(|e| {
                let EntryBody::Message { marks, .. } = &e.body else {
                    return None;
                };
                let pin = marks.pin.as_ref()?;
                (pin.pinned && !marks.deleted && self.visible_message(e)).then_some((e, pin))
            })
            .collect();
        entries.sort_by(|(a, pa), (b, pb)| {
            (if pb.sequence == 0 {
                u64::MAX
            } else {
                pb.sequence
            })
            .cmp(
                &(if pa.sequence == 0 {
                    u64::MAX
                } else {
                    pa.sequence
                }),
            )
            .then_with(|| b.id().cmp(a.id()))
        });
        let revision = event_log::content_hash(
            &entries
                .iter()
                .map(|(e, p)| (e.id(), &p.revision, self.entry_dto(e).text_revision))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| changed())?;
        let offset = if let Some(value) = cursor {
            if value.len() > 512 {
                return Err(changed());
            }
            let parsed: Cursor = serde_json::from_str(value).map_err(|_| changed())?;
            if parsed.revision != revision || parsed.offset > entries.len() {
                return Err(changed());
            }
            parsed.offset
        } else {
            0
        };
        let size = usize::try_from(limit.clamp(1, 100)).map_err(|_| changed())?;
        let end = offset.saturating_add(size).min(entries.len());
        let next = (end < entries.len())
            .then(|| {
                serde_json::to_string(&Cursor {
                    revision: revision.clone(),
                    offset: end,
                })
            })
            .transpose()
            .map_err(|_| changed())?;
        Ok(MessagePinPage {
            revision,
            entries: entries[offset..end]
                .iter()
                .map(|(e, _)| self.entry_dto(e))
                .collect(),
            total: u32::try_from(entries.len()).map_err(|_| changed())?,
            next,
        })
    }
}
