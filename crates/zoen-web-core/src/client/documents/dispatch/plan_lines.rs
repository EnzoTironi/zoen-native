use super::super::{
    browser_id, cents, t, unique_line_mut, valid_line, Client, Command, DocumentResponse, Signer,
};

impl<S: Signer> Client<S> {
    pub(super) fn dispatch_plan_lines(
        &mut self,
        command: Command,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        let mut mutated = false;
        let result = match command {
            Command::AddPlanLine {
                item_id,
                section_index,
                text,
                cost_cents,
                observed_hash,
            } => {
                let cost = cents(&cost_cents)?;
                let text = valid_line(&text, cost)?;
                self.edit_plan_document(
                    &item_id,
                    &observed_hash,
                    &t("Adicionou linha", "Added line"),
                    now,
                    |plan| {
                        plan.sections
                            .get_mut(section_index as usize)
                            .ok_or("Seção indisponível.")?
                            .lines
                            .push(types::PlanLine {
                                id: browser_id("ln", now)?,
                                text,
                                cost_cents: cost,
                                done: false,
                            });
                        Ok(())
                    },
                )
                .inspect(|_| {
                    mutated = true;
                })?
            }
            Command::RemovePlanLine {
                item_id,
                line_id,
                observed_hash,
            } => self
                .edit_plan_document(
                    &item_id,
                    &observed_hash,
                    &t("Removeu linha", "Removed line"),
                    now,
                    |plan| {
                        unique_line_mut(plan, &line_id)?;
                        plan.remove_line(&line_id).ok_or("Linha indisponível.")?;
                        Ok(())
                    },
                )
                .inspect(|_| {
                    mutated = true;
                })?,
            _ => return Err("document operation routed to incompatible handler".into()),
        };
        self.finish_document_response(result, mutated, Vec::new())
    }
}
