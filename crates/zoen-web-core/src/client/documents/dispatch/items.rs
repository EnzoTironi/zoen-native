use super::super::{json, Client, Command, DocumentResponse, ItemContent, ItemKind, Signer, Value};

impl<S: Signer> Client<S> {
    pub(super) fn dispatch_items(
        &mut self,
        command: Command,
        _now: i64,
    ) -> Result<DocumentResponse, String> {
        let mut downloads = Vec::new();
        let result = match command {
            Command::Items {} => {
                let mut items = self.document_items()?.into_values().collect::<Vec<_>>();
                items.sort_by_key(|item| std::cmp::Reverse(item.current().event.at_ms));
                Value::Array(
                    items
                        .iter()
                        .map(|item| self.item_value(item, None, &mut downloads))
                        .collect::<Result<_, _>>()?,
                )
            }
            Command::Item { item_id } => {
                self.item_value(&self.document_item(&item_id)?, None, &mut downloads)?
            }
            Command::ItemDiscussion { space_id, item_id } => {
                self.item_discussion_value(&space_id, &item_id)?
            }
            Command::ItemAt { item_id, version } => self.item_value(
                &self.document_item(&item_id)?,
                Some(version),
                &mut downloads,
            )?,
            Command::FileManifest { item_id, version } => {
                let item = self.document_item(&item_id)?;
                if item.kind == ItemKind::App {
                    self.app_document_access(&item.space)?;
                }
                let number = version.unwrap_or(
                    u32::try_from(item.versions.len())
                        .map_err(|_| "document version limit exceeded")?,
                );
                let current = number
                    .checked_sub(1)
                    .and_then(|index| item.versions.get(index as usize))
                    .ok_or("Versão indisponível.")?;
                if !matches!(
                    (item.kind, &current.content),
                    (ItemKind::File, ItemContent::File(_)) | (ItemKind::App, ItemContent::App(_))
                ) {
                    return Err("Esta versão não contém mídia.".into());
                }
                let document = current
                    .content
                    .owned_file()
                    .ok_or("Esta versão não contém mídia.")?;
                let detail = self.item_value(&item, Some(number), &mut downloads)?;
                json!({"itemId":item.id,"version":number,"doc":document,"info":detail["file"]})
            }
            _ => return Err("document operation routed to incompatible handler".into()),
        };
        self.finish_document_response(result, false, downloads)
    }
}
