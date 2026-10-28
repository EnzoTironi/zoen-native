use super::super::{
    browser_id, error, json, Block, Client, Command, DocumentResponse, Page, Signer,
};

impl<S: Signer> Client<S> {
    pub(super) fn dispatch_utility(
        &mut self,
        command: Command,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        let result = match command {
            Command::NewMutationId {} => json!(browser_id("client_edit", now)?),
            Command::PageParseMarkdown { markdown } => json!(Page::from_markdown(&markdown)
                .map_err(error)?
                .blocks()
                .iter()
                .map(Block::from)
                .collect::<Vec<_>>()),
            Command::PageBlockMarkdown { block } => {
                let page = Page::new();
                page.apply(std::slice::from_ref(&block.id), &[block.edit(None)?])
                    .map_err(error)?;
                json!(page
                    .canonical_markdown()
                    .map_err(|error| error.to_string())?)
            }
            _ => return Err("document operation routed to incompatible handler".into()),
        };
        self.finish_document_response(result, false, Vec::new())
    }
}
