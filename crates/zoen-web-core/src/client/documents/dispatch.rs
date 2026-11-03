//! Document dispatch on the existing signed client and encrypted journal.
use super::{
    apps, error, ChunkRef, Client, Command, Deserialize, DocumentResponse, Signer, Value,
    MAX_REQUEST_BYTES,
};

mod items;
mod page_edits;
mod pages;
mod plan_edits;
mod plan_lines;
mod plan_restore;
mod restore;
mod utility;

#[derive(Deserialize)]
struct Operation<'a> {
    op: &'a str,
}

impl<S: Signer> Client<S> {
    ///
    /// # Errors
    /// Returns an error for malformed requests, unavailable documents, missing
    /// write authority, invalid or conflicting edits, or failed document export,
    /// signing, encryption or client update.
    pub fn document_request(
        &mut self,
        request: &str,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        self.view_now_ms = now;
        if request.len() > apps::MAX_REQUEST_BYTES {
            return Err("Edição grande demais.".into());
        }
        let operation: Operation<'_> = serde_json::from_str(request).map_err(error)?;
        if matches!(
            operation.op,
            "app_document_read" | "app_document_save" | "app_document_copy"
        ) {
            return self.drawing_request(request, now);
        }
        if request.len() > MAX_REQUEST_BYTES {
            return Err("Edição grande demais.".into());
        }
        let command: Command = serde_json::from_str(request).map_err(error)?;
        match command {
            command @ (Command::NewMutationId { .. }
            | Command::PageParseMarkdown { .. }
            | Command::PageBlockMarkdown { .. }) => self.dispatch_utility(command, now),
            command @ (Command::Items { .. }
            | Command::Item { .. }
            | Command::ItemDiscussion { .. }
            | Command::ItemAt { .. }
            | Command::FileManifest { .. }) => self.dispatch_items(command, now),
            command @ (Command::Page { .. }
            | Command::PageAt { .. }
            | Command::PageExportVersion { .. }
            | Command::PageMarkdown { .. }
            | Command::PlanPage { .. }) => self.dispatch_pages(command, now),
            command @ (Command::PageCreate { .. }
            | Command::PageImportMarkdown { .. }
            | Command::PageApply { .. }
            | Command::PageCommit { .. }) => self.dispatch_page_edits(command, now),
            command @ (Command::TogglePlanLine { .. } | Command::EditPlanLine { .. }) => {
                self.dispatch_plan_edits(command, now)
            }
            command @ (Command::AddPlanLine { .. } | Command::RemovePlanLine { .. }) => {
                self.dispatch_plan_lines(command, now)
            }
            command @ (Command::RestorePlanVersion { .. } | Command::Undo { .. }) => {
                self.dispatch_plan_restore(command, now)
            }
            command @ Command::RestoreVersion { .. } => self.dispatch_restore(command, now),
        }
    }

    fn finish_document_response(
        &mut self,
        result: Value,
        mutated: bool,
        mut downloads: Vec<ChunkRef>,
    ) -> Result<DocumentResponse, String> {
        downloads.sort_by(|a, b| a.sha256.cmp(&b.sha256));
        downloads.dedup_by(|a, b| a.sha256 == b.sha256);
        let uploads = if mutated {
            self.pending_document_uploads()
        } else {
            Vec::new()
        };
        Ok(DocumentResponse {
            result,
            update: if mutated {
                Some(self.update(Vec::new())?)
            } else {
                None
            },
            downloads,
            uploads,
        })
    }
}
