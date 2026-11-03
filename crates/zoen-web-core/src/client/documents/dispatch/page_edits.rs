use super::super::{error, json, t, Client, Command, DocumentResponse, Page, Signer};

impl<S: Signer> Client<S> {
    pub(super) fn dispatch_page_edits(
        &mut self,
        command: Command,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        let mut downloads = Vec::new();
        let mutated;
        let result = match command {
            Command::PageCreate { space_id, title } => {
                self.document_writer(&space_id)?;
                let page = Page::from_markdown(&if title.trim().is_empty() {
                    String::new()
                } else {
                    format!("# {}\n", title.trim())
                })
                .map_err(error)?;
                let item = self.create_page(&space_id, "", &page, &t("Criado", "Created"), now)?;
                mutated = true;
                self.item_value(&self.document_item(&item)?, None, &mut downloads)?
            }
            Command::PageImportMarkdown {
                space_id,
                path,
                markdown,
            } => {
                self.document_writer(&space_id)?;
                if path.contains('\0')
                    || path.starts_with('/')
                    || path.split('/').any(|part| part == "..")
                {
                    return Err("Caminho inválido.".into());
                }
                let page = Page::from_markdown(&markdown).map_err(error)?;
                let item =
                    self.create_page(&space_id, &path, &page, &t("Importado", "Imported"), now)?;
                mutated = true;
                self.item_value(&self.document_item(&item)?, None, &mut downloads)?
            }
            Command::PageApply {
                item_id,
                mutation_id,
                edit_context,
                order,
                changed,
            } => {
                let item = self.document_item(&item_id)?;
                let result =
                    self.apply_page(&item, &mutation_id, &edit_context, &order, &changed)?;
                mutated = true;
                result
            }
            Command::PageCommit { item_id, note } => {
                let item = self.document_item(&item_id)?;
                let committed = self.commit_page(&item, &note, now)?;
                mutated = committed;
                json!(committed)
            }
            _ => return Err("document operation routed to incompatible handler".into()),
        };
        self.finish_document_response(result, mutated, downloads)
    }
}
