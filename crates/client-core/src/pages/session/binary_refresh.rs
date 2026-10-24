//! Binary refresh operations on the original PageSession state.

use super::*;

impl PageSession {
    pub(super) fn refresh_binary(&mut self, engine: &Engine) -> R<()> {
        let it = engine.item_state(&self.item)?;
        if it.kind != ItemKind::Page {
            return Err(invalid("the BinaryPage item kind changed"));
        }
        let reusable = self.binary_history.as_ref().filter(|history| {
            Arc::ptr_eq(&history.epoch, &engine.pages.epoch)
                && history.space == it.space
                && history.origin == it.origin
                && history.applied <= it.versions.len()
        });
        let loaded = engine.load_page_journal(it);
        if self.blocked.is_none()
            && reusable.is_some_and(|history| history.applied == it.versions.len())
        {
            if let (
                Ok(loaded),
                SessionJournal::BinaryPage(current),
                SessionDocument::BinaryPage(live),
                SessionDocument::BinaryPage(shadow),
            ) = (&loaded, &self.journal, &self.live, &self.shadow)
            {
                // Authenticate the current journal before reusing memory. A clean
                // no-op still takes the acknowledgement path below exactly once.
                if loaded == current.as_ref()
                    && (!loaded.has_draft || !live.editor_content_eq(shadow))
                {
                    return Ok(());
                }
            }
        }
        let (mut shadow, mut applied, mut frontiers) = match (&self.shadow, reusable) {
            (SessionDocument::BinaryPage(shadow), Some(history)) => (
                shadow
                    .at(&shadow.frontiers())
                    .map_err(super::super::doc_err)?,
                history.applied,
                history.frontiers.clone(),
            ),
            _ => (Page::new(), 0, Vec::new()),
        };
        let mut blocked = None;
        for (index, version) in it.versions.iter().enumerate().skip(applied) {
            if let ItemContent::Page(page) = &version.content {
                if page.manifest.is_some() {
                    return Err(invalid("BinaryPage does not accept a Plan manifest"));
                }
                let Some(bytes) = engine.page_payload(page) else {
                    blocked = Some(SessionBlock::Waiting("the Page is still arriving".into()));
                    break;
                };
                let imported = shadow
                    .at(&shadow.frontiers())
                    .map_err(super::super::doc_err)?;
                if let Err(error) = imported.import(&bytes) {
                    blocked = Some(SessionBlock::Waiting(error.to_string()));
                    break;
                }
                shadow = imported;
            }
            frontiers.push(shadow.frontiers());
            applied = index + 1;
        }
        let history = BinaryHistory {
            epoch: engine.pages.epoch.clone(),
            space: it.space.clone(),
            origin: it.origin.clone(),
            applied,
            frontiers,
        };
        let mut journal = match loaded {
            Ok(journal) => journal,
            Err(error) => {
                self.lock_binary(shadow, history, error.to_string())?;
                return Ok(());
            }
        };
        let live = shadow
            .at(&shadow.frontiers())
            .map_err(super::super::doc_err)?;
        if let Some(snapshot) = &journal.snapshot {
            let result = base64::engine::general_purpose::STANDARD
                .decode(snapshot)
                .map_err(|_| invalid("invalid binary draft"))
                .and_then(|bytes| live.import(&bytes).map_err(super::super::doc_err));
            if let Err(error) = result {
                self.lock_binary(shadow, history, error.to_string())?;
                return Ok(());
            }
        }
        // This branch keeps BinaryPageV1's existing acknowledgement rule and formats.
        // The stronger exact-source semantic fence belongs only to the new Plan tag.
        if blocked.is_none()
            && journal.has_draft
            && live.editor_content_eq(shadow)
            && self.write_author(engine).is_ok()
        {
            let acknowledged = journal.commit.as_ref().is_some_and(|id| {
                engine
                    .logs
                    .get(&it.space)
                    .is_some_and(|log| log.events().iter().any(|event| &event.client_id == id))
            });
            let pending = self.pending(engine)?;
            let queue = engine.store.outbox()?;
            let failed = journal
                .commit
                .as_ref()
                .is_some_and(|id| queue.iter().any(|p| &p.event.client_id == id && p.failed));
            if acknowledged || (journal.commit.is_none() && !pending && !failed) {
                journal.has_draft = false;
                journal.commit = None;
                engine.save_page_journal(it, &journal)?;
            }
        }
        self.shadow = SessionDocument::BinaryPage(shadow);
        self.binary_history = Some(history);
        self.live = SessionDocument::BinaryPage(live);
        self.journal = SessionJournal::BinaryPage(Box::new(journal));
        self.blocked = blocked;
        Ok(())
    }

    pub(super) fn lock_binary(
        &mut self,
        shadow: Page,
        history: BinaryHistory,
        reason: String,
    ) -> R<()> {
        // A cold reader still sees the saved page. An existing reader retains its
        // last verified live intent while the encrypted bytes remain untouched.
        if self.binary_history.is_none() {
            self.live = SessionDocument::BinaryPage(
                shadow
                    .at(&shadow.frontiers())
                    .map_err(super::super::doc_err)?,
            );
        }
        self.shadow = SessionDocument::BinaryPage(shadow);
        self.binary_history = Some(history);
        self.blocked = Some(SessionBlock::DraftLocked(reason));
        Ok(())
    }

    pub(super) fn binary(&self) -> R<&Page> {
        match &self.live {
            SessionDocument::BinaryPage(document) => Ok(document),
            _ => Err(super::super::invalid(
                "este Item não é uma página",
                "this Item is not a page",
            )),
        }
    }

    pub(super) fn binary_frontier(&self, version: u32) -> R<&Frontiers> {
        version
            .checked_sub(1)
            .and_then(|index| self.binary_history.as_ref()?.frontiers.get(index as usize))
            .ok_or_else(|| super::super::invalid("versão não encontrada", "version not found"))
    }

    pub(in crate::pages) fn binary_at(&self, version: u32) -> R<Page> {
        let SessionDocument::BinaryPage(shadow) = &self.shadow else {
            return Err(super::super::invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        };
        shadow
            .at(self.binary_frontier(version)?)
            .map_err(super::super::doc_err)
    }

    pub(in crate::pages) fn restore_binary(&mut self, engine: &Engine, version: u32) -> R<()> {
        self.refresh(engine)?;
        self.write_author(engine)?;
        self.input_state().require_empty()?;
        if self.blocked.is_some() {
            return Err(super::super::invalid(
                "a página ainda está chegando",
                "the page is still arriving",
            ));
        }
        let SessionJournal::BinaryPage(journal) = &self.journal else {
            return Err(super::super::invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        };
        let live = self
            .binary()?
            .at(&self.binary()?.frontiers())
            .map_err(super::super::doc_err)?;
        live.revert_to(self.binary_frontier(version)?)
            .map_err(super::super::doc_err)?;
        let mut journal = journal.clone();
        journal.invalidate_contexts()?;
        journal.snapshot = Some(encode_bytes(&live.snapshot()));
        journal.has_draft = true;
        journal.commit = None;
        engine.save_page_journal(engine.item_state(&self.item)?, &journal)?;
        self.live = SessionDocument::BinaryPage(live);
        self.journal = SessionJournal::BinaryPage(journal);
        Ok(())
    }

    pub(in crate::pages) fn binary_markdown(&mut self, engine: &Engine) -> R<String> {
        self.refresh(engine)?;
        if matches!(self.blocked, Some(SessionBlock::Waiting(_))) {
            return Err(super::super::invalid(
                "a página ainda está chegando",
                "the page is still arriving",
            ));
        }
        Ok(self.binary()?.to_markdown())
    }

    pub(in crate::pages) fn binary_text(&mut self, engine: &Engine) -> R<String> {
        self.refresh(engine)?;
        let document = self.binary()?;
        let hash = document.content_hash();
        if let Some((cached_hash, text)) = &self.text {
            if cached_hash == &hash {
                return Ok(text.clone());
            }
        }
        let text = document
            .blocks()
            .iter()
            .map(|block| block.plain_text().replace('\u{2028}', " "))
            .collect::<Vec<_>>()
            .join("\n");
        if !matches!(self.blocked, Some(SessionBlock::Waiting(_))) {
            self.text = Some((hash, text.clone()));
        }
        Ok(text)
    }
}
