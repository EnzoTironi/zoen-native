//! Plans on the original Engine and canonical document state.

use super::*;

impl Engine {
    pub(crate) fn plan_snapshot(&self, item: &str) -> R<plan::PlanSnapshot> {
        let it = self.item_state(item)?;
        if it.kind != ItemKind::Plan {
            return Err(invalid(
                "este Item não é um plano",
                "this Item is not a plan",
            ));
        }
        let has_creation = self.logs.get(&it.space).is_some_and(|log| {
            log.events().iter().any(|event| {
                matches!(&event.body, EventBody::ItemCreated { item: id, .. } if id == item)
            })
        });
        if !has_creation {
            if let ItemContent::Plan(document) = &it.current().content {
                return Ok(plan::PlanSnapshot::legacy(document, it.current()));
            }
        }
        self.with_document(item, ItemKind::Plan, |_, session| {
            session.plan_snapshot(self)
        })
    }

    pub(crate) fn require_legacy_plan(&self, item: &str) -> R<()> {
        let fold = self.plan_timeline(item)?;
        let pending_cutover = fold.slots.iter().any(|slot| {
            matches!(slot.acceptance, plan::Acceptance::Pending)
                && matches!(&slot.content, Some(ItemContent::Page(page)) if page.manifest.is_some())
        });
        if fold.frozen_at.is_some() || fold.waiting || pending_cutover {
            return Err(invalid(
                "este plano precisa do editor de página; a edição antiga não foi salva",
                "this Plan requires the page editor; the legacy edit was not saved",
            ));
        }
        Ok(())
    }

    pub(crate) fn plan_total(document: &PlanDoc) -> Option<i64> {
        plan::checked_total(document)
    }
}
