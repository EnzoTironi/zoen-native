use super::super::{
    cents, t, unique_line_mut, valid_line, Client, Command, DocumentResponse, Signer,
};

impl<S: Signer> Client<S> {
    pub(super) fn dispatch_plan_edits(
        &mut self,
        command: Command,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        let mut mutated = false;
        let result = match command {
            Command::TogglePlanLine {
                item_id,
                line_id,
                observed_hash,
            } => self
                .edit_plan_document(
                    &item_id,
                    &observed_hash,
                    &t("Alterou conclusão", "Changed completion"),
                    now,
                    |plan| {
                        let line = unique_line_mut(plan, &line_id)?;
                        line.done = !line.done;
                        Ok(())
                    },
                )
                .inspect(|_| {
                    mutated = true;
                })?,
            Command::EditPlanLine {
                item_id,
                line_id,
                text,
                cost_cents,
                observed_hash,
            } => {
                let cost = cents(&cost_cents)?;
                let text = valid_line(&text, cost)?;
                self.edit_plan_document(
                    &item_id,
                    &observed_hash,
                    &t("Editou linha", "Edited line"),
                    now,
                    |plan| {
                        let line = unique_line_mut(plan, &line_id)?;
                        line.text = text;
                        line.cost_cents = cost;
                        Ok(())
                    },
                )
                .inspect(|_| {
                    mutated = true;
                })?
            }
            _ => return Err("document operation routed to incompatible handler".into()),
        };
        self.finish_document_response(result, mutated, Vec::new())
    }
}
