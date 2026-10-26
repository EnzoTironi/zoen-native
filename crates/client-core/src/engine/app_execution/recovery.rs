//! Accepted, projected App effects are receipts; optimistic and rejected events are not.
use super::*;
impl Engine {
    pub(super) fn accepted_app_tool_receipt(
        &self,
        item: &ItemState,
        tool: &str,
        args: &Value,
        caller: &apps::Actor<'_>,
        device: &str,
    ) -> bool {
        self.logs.get(&item.space).is_some_and(|log| {
            log.events().iter().any(|event| {
                if event.author != caller.id || event.device.as_deref() != Some(device) {
                    return false;
                }
                let EventBody::ItemVersioned {
                    item: changed,
                    content,
                    note,
                } = &event.body
                else {
                    return false;
                };
                let ItemContent::App(accepted) = content else {
                    return false;
                };
                changed == &item.id
                    && item.versions.iter().any(|version| {
                        version.author == event.author
                            && version.at_ms == event.at_ms
                            && version.note == *note
                            && version.content == *content
                    })
                    && serde_json::from_str::<Value>(&accepted.state_json)
                        .ok()
                        .is_some_and(|state| {
                            apps::recover_tool_result(&accepted.app, tool, args, &state, caller)
                                .is_some()
                        })
            })
        })
    }
}
