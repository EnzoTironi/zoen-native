use super::super::{error, tr, Client, Command, DocumentResponse, EventBody, ItemKind, Signer};

impl<S: Signer> Client<S> {
    pub(super) fn dispatch_restore(
        &mut self,
        command: Command,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        let mut downloads = Vec::new();
        let mutated;
        let result = match command {
            Command::RestoreVersion { item_id, version } => {
                let item = self.document_item(&item_id)?;
                if item.kind == ItemKind::Plan {
                    return Err("A restauração de um plano exige o vínculo observado.".into());
                }
                if version == 0 || version as usize > item.versions.len() {
                    return Err("Versão indisponível.".into());
                }
                if item.kind == ItemKind::Page {
                    self.document_writer(&item.space)?;
                    let read = self.read_page(&item, true)?;
                    if !read.ready {
                        return Err("A página ainda está chegando.".into());
                    }
                    let mut past = item.clone();
                    past.versions.truncate(version as usize)?;
                    let target = self.read_page(&past, false)?;
                    if !target.ready {
                        return Err("A versão ainda está chegando.".into());
                    }
                    read.live
                        .revert_to(&target.shadow.frontiers())
                        .map_err(error)?;
                    let draft = self
                        .saved
                        .documents
                        .drafts
                        .entry(item.id.clone())
                        .or_default();
                    draft.origin = item.origin_hash();
                    draft.snapshot = read.live.snapshot();
                    self.commit_page(
                        &item,
                        &tr!("Restaurou a v{version}", "Restored v{version}"),
                        now,
                    )?;
                } else {
                    self.queue_domain_body(
                        &item.space,
                        &EventBody::ItemReverted {
                            item: item_id.clone(),
                            to_version: version,
                            note: tr!("Restaurou a v{version}", "Restored v{version}"),
                        },
                        now,
                        Vec::new(),
                    )?;
                }
                mutated = true;
                self.item_value(&self.document_item(&item_id)?, None, &mut downloads)?
            }
            _ => return Err("document operation routed to incompatible handler".into()),
        };
        self.finish_document_response(result, mutated, downloads)
    }
}
