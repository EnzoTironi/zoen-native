//! Whole operations on the existing Client; no additional state owner.
use super::*;

impl<S: Signer> Client<S> {
    /// A file, photo, video or voice note whose chunks the host already uploaded.
    pub fn queue_file(
        &mut self,
        space: &str,
        doc: types::FileDoc,
        now_ms: i64,
    ) -> Result<Update, String> {
        self.queue_file_inner(space, doc, now_ms)
    }

    /// Explicit signed item/message identities; callers never guess from a filename
    /// or search the latest timeline entry after an asynchronous upload.
    pub fn queue_file_record(
        &mut self,
        space: &str,
        doc: types::FileDoc,
        now_ms: i64,
    ) -> Result<(Update, String, String), String> {
        let before = self.saved.outbox.keys().cloned().collect::<BTreeSet<_>>();
        let update = self.queue_file(space, doc, now_ms)?;
        let (item, message) = self
            .saved
            .outbox
            .iter()
            .filter(|(id, _)| !before.contains(*id))
            .filter_map(|(_, pending)| pending.inner.as_ref())
            .find_map(|event| match &event.body {
                EventBody::ItemCreated {
                    item,
                    content: types::ItemContent::File(_),
                    ..
                } => Some((item.clone(), event.client_id.clone())),
                _ => None,
            })
            .ok_or("The shared file was not queued.")?;
        let message = self
            .saved
            .outbox
            .iter()
            .filter(|(id, _)| !before.contains(*id))
            .filter_map(|(_, pending)| pending.inner.as_ref())
            .find_map(|event| match &event.body {
                EventBody::MessagePosted {
                    attaches: Some(target),
                    ..
                } if *target == item => Some(event.client_id.clone()),
                _ => None,
            })
            .unwrap_or(message);
        Ok((update, item, message))
    }

    /// Persist failed media atomically without ever publishing its incomplete manifest.
    pub fn queue_failed_file(
        &mut self,
        space: &str,
        doc: types::FileDoc,
        now_ms: i64,
        reason: String,
        chunks: BTreeMap<String, Vec<u8>>,
    ) -> Result<Update, String> {
        for chunk in &doc.chunks {
            let sealed = chunks.get(&chunk.blob).ok_or("missing local chunk")?;
            crate::media::open_chunk(chunk, sealed)?;
        }
        if doc.chunks.is_empty() {
            return Err("file empty or too large".into());
        }
        // Failed media is local-only: retain it even while disconnected or before
        // MLS catches up. It must never need encryption or become publishable.
        let mut pending = self.clear_event(
            space,
            EventBody::ItemCreated {
                item: types::new_secret_id("it").map_err(|error| error.to_string())?,
                kind: types::ItemKind::File,
                content: types::ItemContent::File(doc),
                origin: "Adicionado".into(),
            },
            now_ms,
        )?;
        let event = pending
            .envelope()?
            .plain_event()
            .ok_or("invalid inner message")?;
        pending.inner = Some(event.clone());
        pending.failure = Some(reason);
        self.saved.local_chunks.extend(chunks);
        self.saved.outbox.insert(event.client_id.clone(), pending);
        self.update(Vec::new())
    }

    pub fn local_chunk(&self, blob: &str) -> Vec<u8> {
        self.saved
            .local_chunks
            .get(blob)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn queue_file_inner(
        &mut self,
        space: &str,
        mut doc: types::FileDoc,
        now_ms: i64,
    ) -> Result<Update, String> {
        if doc.chunks.is_empty() {
            return Err("file empty or too large".into());
        }
        if let Some(reply) = doc.voice.as_mut().and_then(|voice| voice.reply.as_mut()) {
            let reference = match self.posted(space, &reply.to) {
                Some(Some(root)) if reply.thread && root.thread => root.to,
                Some(_) => reply.to.clone(),
                None => return Err("That message is no longer available.".into()),
            };
            reply.to = self
                .resolve_post(space, &reference)
                .ok_or("That message is no longer available.")?
                .client_id
                .clone();
        }
        let item = types::new_secret_id("it").map_err(|error| error.to_string())?;
        let voice = doc.is_voice();
        let mut bodies = vec![EventBody::ItemCreated {
            item: item.clone(),
            kind: types::ItemKind::File,
            content: types::ItemContent::File(doc),
            origin: "Adicionado".into(),
        }];
        if !voice {
            bodies.push(EventBody::MessagePosted {
                message: format!("msg_attachment_{item}"),
                text: String::new(),
                attaches: Some(item),
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            });
        }
        self.queue_domain_batch(space, bodies, now_ms)?;
        self.update(Vec::new())
    }

    /// Edit, delete for everyone, react or unreact. Same encrypted path as messages.
    /// Keep a queued file/voice in the timeline as failed so the clip is not lost.
    pub fn fail_outbox(
        &mut self,
        client_id: &str,
        reason: impl Into<String>,
    ) -> Result<Update, String> {
        if let Some(pending) = self.saved.outbox.get_mut(client_id) {
            pending.failure = Some(reason.into());
            self.sent.remove(client_id);
        }
        self.update(Vec::new())
    }
}
