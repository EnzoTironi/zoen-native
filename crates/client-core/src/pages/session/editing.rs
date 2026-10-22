//! Editing operations on the original PageSession state.

use super::*;

impl PageSession {
    pub(in crate::pages) fn apply_from(
        &mut self,
        engine: &Engine,
        mutation_id: &str,
        context: &str,
        edit: DocumentEdit<'_>,
    ) -> R<AcceptedEdit> {
        self.refresh(engine)?;
        self.write_author(engine)?;
        self.input_state().require_empty()?;
        if self.blocked.is_some() {
            return Err(invalid("the document is not ready for editing"));
        }
        if mutation_id.is_empty()
            || mutation_id.len() > 128
            || !mutation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(invalid("invalid edit identifier"));
        }
        let result = match edit {
            DocumentEdit::Plan { order, changed } => {
                self.apply_plan(engine, mutation_id, context, order, changed)
            }
            DocumentEdit::BinaryPage { order, changed } => {
                self.apply_binary(engine, mutation_id, context, order, changed, None)
            }
        }?;
        #[cfg(test)]
        if self.lose_apply_response {
            self.lose_apply_response = false;
            return Err(CoreError::Storage {
                message: "injected lost response after durable acceptance".into(),
            });
        }
        Ok(result)
    }

    pub(super) fn apply_plan(
        &mut self,
        engine: &Engine,
        id: &str,
        context: &str,
        order: &[String],
        changed: &[PlanBlockEdit],
    ) -> R<AcceptedEdit> {
        let (DocumentTag::PlanV1(tag), SessionJournal::Plan(journal)) = (&self.tag, &self.journal)
        else {
            return Err(invalid("a Plan edit requires a Plan session"));
        };
        let it = engine.item_state(&self.item)?;
        let decoded = decode_context(engine, it, tag, context)?;
        let doc = plan_doc(&self.live)?;
        let revision = doc
            .verify_revision(&decode_bytes(
                &decoded.revision_base64,
                PlanRevision::MAX_ENCODED_BYTES,
            )?)
            .map_err(plan_error)?;
        let request_hash =
            super::super::drafts::tagged::request_hash(tag, id, context, order, changed)?;
        if let Some(receipt) = journal.receipt(id) {
            if receipt.request_hash != request_hash {
                return Err(stale());
            }
            verify_receipt(engine, it, tag, doc, receipt)?;
            return Ok(AcceptedEdit {
                mutation_id: id.into(),
                request_hash,
                applied_content_hash: receipt.content_hash.clone(),
                applied_edit_context: receipt.applied_edit_context.clone(),
                replayed: true,
                current: self.read_current(engine)?,
            });
        }
        if decoded.generation < journal.minimum_generation {
            return Err(stale());
        }
        if decoded.generation > journal.generation {
            return Err(invalid("future editing context"));
        }
        let context_hash = super::super::drafts::tagged::context_hash(context);
        let mut desired = doc.at(&revision).map_err(plan_error)?;
        let (observed, base) = if let Some(prior) = journal.continuation(&context_hash) {
            desired
                .apply_from(&revision, order, changed)
                .map_err(plan_error)?;
            let previous = verify_receipt(engine, it, tag, doc, prior)?;
            let mut observed = doc.at(&previous).map_err(plan_error)?;
            let base = observed.version();
            let complete: Vec<_> = desired
                .blocks()
                .map_err(plan_error)?
                .into_iter()
                .map(|block| PlanBlockEdit {
                    text: block.plain_text(),
                    spans: documents::spans_of(&block.runs),
                    id: block.id,
                    content: block.content,
                })
                .collect();
            observed
                .apply_from(&previous, order, &complete)
                .map_err(plan_error)?;
            (observed, base)
        } else {
            let base = desired.version();
            desired
                .apply_from(&revision, order, changed)
                .map_err(plan_error)?;
            (desired, base)
        };
        let accepted_hash = observed.semantic_hash().map_err(plan_error)?;
        let accepted_revision = observed
            .revision()
            .map_err(plan_error)?
            .encode()
            .map_err(plan_error)?;
        let mut live = doc.fork().map_err(plan_error)?;
        live.import(&observed.updates_since(&base).map_err(plan_error)?)
            .map_err(plan_error)?;
        live.blocks().map_err(plan_error)?;
        let merged_revision = live
            .revision()
            .map_err(plan_error)?
            .encode()
            .map_err(plan_error)?;
        let mut staged = journal.clone();
        let generation = staged.next_generation()?;
        let accepted_context =
            super::super::drafts::tagged::context(engine, it, tag, generation, &accepted_revision)?;
        // A response's current view must also have a representable context before the
        // durable acceptance. The receipt retains the separate own-branch context.
        super::super::drafts::tagged::context(engine, it, tag, generation, &merged_revision)?;
        staged.accept(PlanReceiptV1 {
            tag: tag.clone(),
            mutation_id: id.into(),
            request_hash: request_hash.clone(),
            context_hash,
            input_generation: decoded.generation,
            accepted_generation: generation,
            content_hash: accepted_hash.clone(),
            accepted_revision_base64: encode_bytes(&accepted_revision),
            applied_edit_context: accepted_context.clone(),
        })?;
        staged.snapshot = Some(PlanSnapshotV1 {
            prefix: engine.plan_timeline(&self.item)?.manifest()?,
            update_json_base64: encode_bytes(&live.full_update().to_json().map_err(plan_error)?),
        });
        staged.has_draft = true;
        // Preserve an older in-flight publication while newer local intent is accepted.
        super::super::drafts::tagged::save(engine, it, &staged)?;
        self.live = SessionDocument::Plan(live);
        self.journal = SessionJournal::Plan(staged);
        Ok(AcceptedEdit {
            mutation_id: id.into(),
            request_hash,
            applied_content_hash: accepted_hash,
            applied_edit_context: accepted_context,
            replayed: false,
            current: self.read_current(engine)?,
        })
    }

    pub(super) fn apply_binary(
        &mut self,
        engine: &Engine,
        id: &str,
        context: &str,
        order: &[String],
        changed: &[PageBlockDto],
        input_fingerprint: Option<String>,
    ) -> R<AcceptedEdit> {
        let (SessionDocument::BinaryPage(doc), SessionJournal::BinaryPage(journal)) =
            (&self.live, &self.journal)
        else {
            return Err(invalid("a binary edit requires a BinaryPage session"));
        };
        if context.len() > 2 * 1024 * 1024 {
            return Err(invalid("invalid binary editing context"));
        }
        let it = engine.item_state(&self.item)?;
        let decoded = engine.decode_page_edit_context(it, context)?;
        let request_hash = drafts::request_hash(id, context, order, changed)?;
        if let Some(receipt) = journal.receipt(id) {
            if receipt.fingerprint != request_hash {
                return Err(stale());
            }
            return Ok(AcceptedEdit {
                mutation_id: id.into(),
                request_hash,
                applied_content_hash: receipt.content_hash.clone(),
                applied_edit_context: engine.page_context_at(
                    it,
                    receipt.accepted_generation,
                    receipt.frontiers.clone(),
                )?,
                replayed: true,
                current: self.read_current(engine)?,
            });
        }
        if decoded.generation < journal.minimum_generation {
            return Err(stale());
        }
        if decoded.generation > journal.generation {
            return Err(invalid("future binary editing context"));
        }
        let revision = Frontiers::decode(&decoded.frontiers).map_err(plan_error)?;
        let desired = doc.at(&revision).map_err(super::super::doc_err)?;
        let current: std::collections::HashMap<_, _> = desired
            .blocks()
            .into_iter()
            .map(|b| (b.id, b.kind))
            .collect();
        let edits: Vec<_> = changed
            .iter()
            .map(|dto| BlockEdit {
                id: dto.id.clone(),
                kind: super::super::kind_from_dto(dto, current.get(&dto.id)),
                text: dto.text.clone(),
                spans: dto
                    .spans
                    .iter()
                    .map(|span| documents::Span {
                        start: span.start,
                        end: span.end,
                        key: span.key.clone(),
                        value: span.value.clone(),
                    })
                    .collect(),
            })
            .collect();
        let context_hash = drafts::context_hash(context);
        let (observed, base) = if let Some(prior) = journal.continuation(&context_hash) {
            desired
                .apply(order, &edits)
                .map_err(super::super::doc_err)?;
            let previous = Frontiers::decode(&prior.frontiers).map_err(plan_error)?;
            let observed = doc.at(&previous).map_err(super::super::doc_err)?;
            let base = observed.version();
            let complete: Vec<_> = desired
                .blocks()
                .into_iter()
                .map(|b| BlockEdit {
                    text: b.plain_text(),
                    spans: documents::spans_of(&b.runs),
                    id: b.id,
                    kind: b.kind,
                })
                .collect();
            observed
                .apply(order, &complete)
                .map_err(super::super::doc_err)?;
            (observed, base)
        } else {
            let base = desired.version();
            desired
                .apply(order, &edits)
                .map_err(super::super::doc_err)?;
            (desired, base)
        };
        let live = doc.at(&doc.frontiers()).map_err(super::super::doc_err)?;
        live.import(&observed.updates_since(&base))
            .map_err(super::super::doc_err)?;
        let hash = observed.content_hash();
        let fronts = observed.frontiers().encode();
        let mut journal = journal.clone();
        journal.accept(
            id.into(),
            request_hash.clone(),
            context_hash,
            hash.clone(),
            decoded.generation,
            fronts.clone(),
        )?;
        journal.snapshot = Some(encode_bytes(&live.snapshot()));
        journal.has_draft = true;
        let accepted_context = engine.page_context_at(it, journal.generation, fronts)?;
        if let Some(fingerprint) = input_fingerprint {
            journal.input.capture = None;
            journal.input.binding = None;
            journal.input.accept(
                fingerprint,
                Some(super::super::input::Applied {
                    mutation_id: id.into(),
                    mutation_hash: request_hash.clone(),
                    content_hash: hash.clone(),
                    context: accepted_context.clone(),
                }),
            )?;
        }
        engine.save_page_journal(it, &journal)?;
        self.live = SessionDocument::BinaryPage(live);
        self.journal = SessionJournal::BinaryPage(journal);
        Ok(AcceptedEdit {
            mutation_id: id.into(),
            request_hash,
            applied_content_hash: hash,
            applied_edit_context: accepted_context,
            replayed: false,
            current: self.read_current(engine)?,
        })
    }
}
