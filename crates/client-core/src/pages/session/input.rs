//! Input operations on the original PageSession state.

use super::*;

impl PageSession {
    pub(in crate::pages) fn input_state(&self) -> &super::super::input::InputStateV1 {
        match &self.journal {
            SessionJournal::BinaryPage(j) => &j.input,
            SessionJournal::Plan(j) => &j.input,
        }
    }

    pub(in crate::pages) fn input_ready(&mut self, engine: &Engine) -> R<()> {
        self.refresh(engine)?;
        self.write_author(engine)?;
        if self.blocked.is_some() {
            return Err(invalid("the document is not ready for native input"));
        }
        Ok(())
    }

    pub(super) fn save_input(
        &mut self,
        engine: &Engine,
        state: super::super::input::InputStateV1,
    ) -> R<()> {
        state.validate()?;
        let it = engine.item_state(&self.item)?;
        match &self.journal {
            SessionJournal::BinaryPage(j) => {
                let mut staged = j.clone();
                staged.input = state;
                engine.save_page_journal(it, &staged)?;
                self.journal = SessionJournal::BinaryPage(staged);
            }
            SessionJournal::Plan(j) => {
                let mut staged = j.clone();
                staged.input = state;
                super::super::drafts::tagged::save(engine, it, &staged)?;
                self.journal = SessionJournal::Plan(staged);
            }
        }
        Ok(())
    }

    pub(in crate::pages) fn input_open(
        &mut self,
        engine: &Engine,
        revision: u64,
        writer: &str,
        target: super::super::input::PageInputTarget,
        mode: super::super::input::PageInputMode,
        context: &str,
    ) -> R<super::super::input::PageInputDto> {
        use super::super::input::*;
        validate_writer(writer)?;
        validate_target(&target)?;
        if context.len() > MAX_CONTEXT_BYTES || self.input_state().revision != revision {
            return Err(invalid("stale input revision or oversized context"));
        }
        let mut state = self.input_state().clone();
        if state.capture.is_some() {
            let b = state
                .binding
                .as_mut()
                .ok_or_else(|| invalid("missing input binding"))?;
            if b.target != target || b.mode != mode {
                return Err(invalid("recover the existing input target and mode first"));
            }
            b.writer_id = writer.into();
        } else {
            let it = engine.item_state(&self.item)?;
            let base_block = match (&self.tag, &self.live, &self.journal) {
                (
                    DocumentTag::BinaryPageV1,
                    SessionDocument::BinaryPage(doc),
                    SessionJournal::BinaryPage(j),
                ) => {
                    let c = engine.decode_page_edit_context(it, context)?;
                    if c.generation < j.minimum_generation || c.generation > j.generation {
                        return Err(stale());
                    }
                    let base = doc
                        .at(&Frontiers::decode(&c.frontiers).map_err(plan_error)?)
                        .map_err(super::super::doc_err)?;
                    match &target {
                        PageInputTarget::AppendNew => None,
                        PageInputTarget::Block { block_id } => {
                            Some(TaggedBlock::Page(super::super::block_dto(
                                &base
                                    .blocks()
                                    .into_iter()
                                    .find(|b| b.id == *block_id)
                                    .ok_or_else(|| {
                                        invalid("input block is not in the observed context")
                                    })?,
                            )))
                        }
                    }
                }
                (DocumentTag::PlanV1(tag), SessionDocument::Plan(doc), SessionJournal::Plan(j)) => {
                    let c = decode_context(engine, it, tag, context)?;
                    if c.generation < j.minimum_generation || c.generation > j.generation {
                        return Err(stale());
                    }
                    let revision = doc
                        .verify_revision(&decode_bytes(
                            &c.revision_base64,
                            PlanRevision::MAX_ENCODED_BYTES,
                        )?)
                        .map_err(plan_error)?;
                    let base = doc.at(&revision).map_err(plan_error)?;
                    match &target {
                        PageInputTarget::AppendNew => None,
                        PageInputTarget::Block { block_id } => Some(TaggedBlock::Plan(
                            (&base
                                .blocks()
                                .map_err(plan_error)?
                                .into_iter()
                                .find(|b| b.id == *block_id)
                                .ok_or_else(|| {
                                    invalid("input Plan block is not in the observed context")
                                })?)
                                .into(),
                        )),
                    }
                }
                _ => return Err(invalid("input document tag mismatch")),
            };
            state.binding = Some(Binding {
                writer_id: writer.into(),
                target,
                base_edit_context: context.into(),
                base_block,
                mode,
            });
        }
        state.next()?;
        self.save_input(engine, state)?;
        self.input_state().dto()
    }

    pub(in crate::pages) fn input_checkpoint(
        &mut self,
        engine: &Engine,
        writer: &str,
        revision: u64,
        sequence: u64,
        payload: &[u8],
    ) -> R<super::super::input::PageInputAck> {
        use super::super::input::*;
        if payload.len() > MAX_CAPTURE {
            return Err(invalid("input capture exceeds 1 MiB"));
        }
        let fingerprint =
            fingerprint(&("checkpoint", writer, revision, sequence, digest(payload)))?;
        if let Some(receipt) = self.input_state().replay(&fingerprint) {
            return Ok(PageInputAck {
                accepted_revision: receipt.accepted_revision,
                current: self.input_state().dto()?,
                replayed: true,
            });
        }
        let binding = self.input_state().check(writer, revision)?;
        if self
            .input_state()
            .capture
            .as_ref()
            .is_some_and(|c| sequence <= c.sequence)
        {
            return Err(invalid("input sequence must increase"));
        }
        validate_payload(&binding.mode, sequence, payload)?;
        let mut state = self.input_state().clone();
        state.capture = Some(Capture {
            sequence,
            payload: payload.into(),
            sha256: digest(payload),
        });
        state.accept(fingerprint, None)?;
        self.save_input(engine, state)?;
        Ok(PageInputAck {
            accepted_revision: self.input_state().revision,
            current: self.input_state().dto()?,
            replayed: false,
        })
    }

    pub(in crate::pages) fn input_discard(
        &mut self,
        engine: &Engine,
        writer: &str,
        revision: u64,
        sequence: u64,
        sha: &str,
        id: &str,
    ) -> R<super::super::input::PageInputAck> {
        use super::super::input::*;
        validate_writer(id)?;
        let fingerprint = fingerprint(&("discard", writer, revision, sequence, sha, id))?;
        if let Some(receipt) = self.input_state().replay(&fingerprint) {
            return Ok(PageInputAck {
                accepted_revision: receipt.accepted_revision,
                current: self.input_state().dto()?,
                replayed: true,
            });
        }
        self.input_state()
            .consume(writer, revision, sequence, sha)?;
        let mut state = self.input_state().clone();
        state.capture = None;
        state.binding = None;
        state.accept(fingerprint, None)?;
        self.save_input(engine, state)?;
        Ok(PageInputAck {
            accepted_revision: self.input_state().revision,
            current: self.input_state().dto()?,
            replayed: false,
        })
    }

    pub(in crate::pages) fn input_release_empty(
        &mut self,
        engine: &Engine,
        writer: &str,
        revision: u64,
        id: &str,
    ) -> R<super::super::input::PageInputAck> {
        use super::super::input::*;
        validate_writer(writer)?;
        validate_writer(id)?;
        let fingerprint = fingerprint(&("release-empty", writer, revision, id))?;
        if let Some(receipt) = self.input_state().replay(&fingerprint) {
            return Ok(PageInputAck {
                accepted_revision: receipt.accepted_revision,
                current: self.input_state().dto()?,
                replayed: true,
            });
        }
        self.input_state().check(writer, revision)?;
        if self.input_state().capture.is_some() {
            return Err(invalid("captured input requires an exact Apply or Discard"));
        }
        let mut state = self.input_state().clone();
        state.binding = None;
        state.accept(fingerprint, None)?;
        self.save_input(engine, state)?;
        Ok(PageInputAck {
            accepted_revision: self.input_state().revision,
            current: self.input_state().dto()?,
            replayed: false,
        })
    }

    pub(in crate::pages) fn input_apply(
        &mut self,
        engine: &Engine,
        writer: &str,
        revision: u64,
        sequence: u64,
        sha: &str,
        id: &str,
    ) -> R<(super::super::input::PageInputAck, AcceptedEdit)> {
        use super::super::input::*;
        validate_writer(id)?;
        let fingerprint = fingerprint(&("apply", writer, revision, sequence, sha, id))?;
        if let Some(receipt) = self.input_state().replay(&fingerprint) {
            let a = receipt
                .applied
                .as_ref()
                .ok_or_else(|| invalid("missing accepted input edit"))?;
            return Ok((
                PageInputAck {
                    accepted_revision: receipt.accepted_revision,
                    current: self.input_state().dto()?,
                    replayed: true,
                },
                AcceptedEdit {
                    mutation_id: a.mutation_id.clone(),
                    request_hash: a.mutation_hash.clone(),
                    applied_content_hash: a.content_hash.clone(),
                    applied_edit_context: a.context.clone(),
                    replayed: true,
                    current: self.read_current(engine)?,
                },
            ));
        }
        // Public canonical Plan writes retain their existing closed gate.
        if !matches!(self.tag, DocumentTag::BinaryPageV1) {
            return Err(invalid("canonical Plan input Apply is not admitted"));
        }
        let binding = self
            .input_state()
            .consume(writer, revision, sequence, sha)?
            .clone();
        let capture = self
            .input_state()
            .capture
            .as_ref()
            .ok_or_else(|| invalid("no captured input"))?;
        let mut changed = page_edits(&binding, capture)?;
        let it = engine.item_state(&self.item)?;
        let decoded = engine.decode_page_edit_context(it, &binding.base_edit_context)?;
        let base = self
            .binary()?
            .at(&Frontiers::decode(&decoded.frontiers).map_err(plan_error)?)
            .map_err(super::super::doc_err)?;
        let mut order: Vec<String> = base.blocks().into_iter().map(|b| b.id).collect();
        let at = match &binding.target {
            PageInputTarget::AppendNew => order.len(),
            PageInputTarget::Block { block_id } => {
                let i = order
                    .iter()
                    .position(|id| id == block_id)
                    .ok_or_else(|| invalid("input target is absent from its base"))?;
                order.remove(i);
                if let Some(first) = changed.first_mut() {
                    first.id = block_id.clone();
                }
                i
            }
        };
        for (offset, b) in changed.iter().enumerate() {
            order.insert(at + offset, b.id.clone());
        }
        if let SessionJournal::BinaryPage(j) = &self.journal {
            if j.receipt(id).is_some() {
                return Err(invalid("input mutation id was already used"));
            }
        }
        let edit = self.apply_binary(
            engine,
            id,
            &binding.base_edit_context,
            &order,
            &changed,
            Some(fingerprint),
        )?;
        #[cfg(test)]
        if self.lose_apply_response {
            self.lose_apply_response = false;
            return Err(crate::CoreError::Storage {
                message: "injected lost response after durable input acceptance".into(),
            });
        }
        Ok((
            PageInputAck {
                accepted_revision: self.input_state().revision,
                current: self.input_state().dto()?,
                replayed: false,
            },
            edit,
        ))
    }
}
