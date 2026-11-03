use super::super::{json, Client, Command, DocumentResponse, Signer, Value};

impl<S: Signer> Client<S> {
    pub(super) fn dispatch_pages(
        &mut self,
        command: Command,
        _now: i64,
    ) -> Result<DocumentResponse, String> {
        let mut downloads = Vec::new();
        let result = match command {
            Command::Page { item_id } => {
                let item = self.document_item(&item_id)?;
                let read = self.read_page(&item, true)?;
                downloads.extend(read.downloads.clone());
                self.page_value(&item, &read, false)?
            }
            Command::PageAt { item_id, version } => {
                let mut item = self.document_item(&item_id)?;
                if version == 0 || version as usize > item.versions.len() {
                    return Err("Versão indisponível.".into());
                }
                item.versions.truncate(version as usize)?;
                let read = self.read_page(&item, false)?;
                downloads.extend(read.downloads.clone());
                self.page_value(&item, &read, true)?
            }
            Command::PageExportVersion {
                space_id,
                item_id,
                version,
                observed_hash,
            } => self.page_export_version_value(&space_id, &item_id, version, &observed_hash)?,
            Command::PageMarkdown { item_id } => {
                let item = self.document_item(&item_id)?;
                let read = self.read_page(&item, true)?;
                downloads.extend(read.downloads.clone());
                if !read.ready {
                    return Ok(DocumentResponse {
                        result: Value::Null,
                        update: None,
                        downloads,
                        uploads: Vec::new(),
                    });
                }
                json!(read.live.to_markdown().map_err(|error| error.to_string())?)
            }
            Command::PlanPage { item_id } => {
                self.plan_page_value(&self.document_item(&item_id)?, &mut downloads)?
            }
            _ => return Err("document operation routed to incompatible handler".into()),
        };
        self.finish_document_response(result, false, downloads)
    }
}
