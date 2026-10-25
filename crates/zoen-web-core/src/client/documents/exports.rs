//! A version/hash-bound export of verified shared history, with no live draft.
use super::*;

impl<S: Signer> Client<S> {
    pub(super) fn page_export_version_value(
        &self,
        space: &str,
        id: &str,
        version: u32,
        observed: &str,
    ) -> Result<Value, String> {
        if !self.roster(space).contains_key(&self.account.identity) {
            return Err(t(
                "Só membros exportam esta página.",
                "Only members can export this page.",
            ));
        }
        if let Some(reason) = self.saved.integrity_errors.get(space) {
            return Err(reason.clone());
        }
        let mut item = self.document_item(id)?;
        if item.space != space || item.versions[0].failure.is_some() {
            return Err(t(
                "Arquivo indisponível nesta conversa.",
                "Artifact unavailable in this conversation.",
            ));
        }
        if version == 0 || version as usize > item.versions.len() {
            return Err(t("Versão indisponível.", "Version unavailable."));
        }
        item.versions.truncate(version as usize);
        if item.versions.iter().any(|v| v.failure.is_some()) {
            return Err(t(
                "Esta versão não pôde ser compartilhada.",
                "This version could not be shared.",
            ));
        }
        let read = self.read_page(&item, false)?;
        if !read.ready {
            return Err(read.error.unwrap_or_else(|| {
                t(
                    "A página ainda está chegando.",
                    "The page is still arriving.",
                )
            }));
        }
        let hash = read
            .shadow
            .content_hash()
            .map_err(|error| error.to_string())?;
        if hash != observed {
            return Err(t(
                "Esta versão mudou; abra a prévia novamente.",
                "This version changed; open its preview again.",
            ));
        }
        let ItemContent::Page(content) = &item.current().content else {
            return Err("Esta versão não é uma página.".into());
        };
        Ok(
            json!({ "item_id":id, "space_id":space, "version":version, "title":content.title,
            "content_hash":hash, "markdown":read.shadow.to_markdown().map_err(|error| error.to_string())? }),
        )
    }
}
