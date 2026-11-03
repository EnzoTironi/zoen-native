use super::super::{t, tr, Client, Command, DocumentResponse, Signer};

impl<S: Signer> Client<S> {
    pub(super) fn dispatch_plan_restore(
        &mut self,
        command: Command,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        let mut downloads = Vec::new();
        let mutated;
        let result = match command {
            Command::RestorePlanVersion {
                item_id,
                version,
                observed_hash,
            } => {
                self.restore_plan(
                    &item_id,
                    version,
                    &observed_hash,
                    &t("Restaurou versão", "Restored version"),
                    now,
                )?;
                mutated = true;
                self.item_value(&self.document_item(&item_id)?, None, &mut downloads)?
            }
            Command::Undo { token } => {
                let item = self.document_item(&token.item_id)?;
                if item.versions.len() != token.result_version as usize {
                    return Err("O plano mudou. Revise antes de desfazer.".into());
                }
                self.restore_plan(
                    &token.item_id,
                    token.restore_version,
                    &token.observed_hash,
                    &tr!("Desfez: {}", "Undid: {}", token.label),
                    now,
                )?;
                mutated = true;
                self.item_value(&self.document_item(&token.item_id)?, None, &mut downloads)?
            }
            _ => return Err("document operation routed to incompatible handler".into()),
        };
        self.finish_document_response(result, mutated, downloads)
    }
}
