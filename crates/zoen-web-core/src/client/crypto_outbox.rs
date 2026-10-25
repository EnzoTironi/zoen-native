//! Whole operations on the existing Client; no additional state owner.
use super::*;

impl<S: Signer> Client<S> {
    pub(super) fn sign_content(
        &self,
        content: Vec<u8>,
        sealed: bool,
    ) -> Result<PendingEnvelope, String> {
        let hash = if sealed {
            event_log::content::signed_hash(&content)
        } else {
            event_log::content::content_hash(&content)
        };
        let message = if sealed {
            event_log::content::signature_message(&content, &hash)
        } else {
            hash.into_bytes()
        };
        let sig = hex::encode(self.signer.sign(&message).map_err(|e| format!("{e:?}"))?);
        let pending = PendingEnvelope {
            content,
            sig,
            cert: self.account.cert.clone(),
            inner: None,
            epoch: None,
            failure: None,
            invite: None,
            blob_uploads: Vec::new(),
            order: None,
            after_commit: None,
        };
        pending.envelope()?.verify().map_err(|e| e.to_string())?;
        Ok(pending)
    }

    pub(super) fn clear_event(
        &self,
        space: &str,
        body: EventBody,
        now_ms: i64,
    ) -> Result<PendingEnvelope, String> {
        let content = SignedContent {
            v: 3,
            space: space.into(),
            client_id: types::new_ulid(now_ms),
            author: self.account.identity.clone(),
            device: Some(self.account.device.clone()),
            at_ms: now_ms,
            seen: self.head(space).as_ref().map(Into::into),
            payload: Some(Payload::Body(event_log::content::encode_body(&body))),
        }
        .encode();
        self.sign_content(content, false)
    }

    pub(super) fn seal_inner(&self, event: &Event, epoch: u64) -> Result<PendingEnvelope, String> {
        let plaintext = InnerEvent {
            content: event.content.clone(),
            sig: event.sig.clone(),
        }
        .encode();
        let ciphertext = self
            .device()?
            .seal(&event.space, &plaintext)
            .map_err(|e| e.to_string())?;
        let content = SignedContent {
            v: event_log::content::SEALED_CONTENT_VERSION,
            space: event.space.clone(),
            client_id: event.client_id.clone(),
            author: event.author.clone(),
            device: event.device.clone(),
            at_ms: event.at_ms,
            seen: event.seen.as_ref().map(Into::into),
            payload: Some(Payload::Sealed(Sealed::new(
                SealedKind::Application,
                SUITE_ID,
                ciphertext,
            ))),
        }
        .encode();
        let mut pending = self.sign_content(content, true)?;
        pending.inner = Some(event.clone());
        pending.epoch = Some(epoch);
        Ok(pending)
    }

    pub fn queue_message(
        &mut self,
        space: &str,
        text: &str,
        now_ms: i64,
    ) -> Result<Update, String> {
        self.queue_post(space, text, None, false, now_ms)
    }

    /// A message, optionally an inline reply (`thread == false`) or a thread reply to
    /// `reply`, or forwarded. Thread replies always point at the root, like native.
    pub fn queue_post(
        &mut self,
        space: &str,
        text: &str,
        reply: Option<(String, bool)>,
        forwarded: bool,
        now_ms: i64,
    ) -> Result<Update, String> {
        let reply = match reply {
            Some((to, thread)) => {
                let target = self
                    .posted(space, &to)
                    .ok_or("That message is no longer available.")?;
                let to = match target {
                    Some(root) if thread && root.thread => root.to,
                    _ => to,
                };
                let to = self
                    .resolve_post(space, &to)
                    .map(|event| event.hash.clone())
                    .unwrap_or(to);
                Some(types::ReplyRef {
                    artifact: None,
                    to,
                    thread,
                })
            }
            None => None,
        };
        if text.trim().is_empty() || text.len() > 32_768 {
            return Err("Write a message of up to 32 KB.".into());
        }
        self.queue_domain_body(
            space,
            EventBody::MessagePosted {
                message: types::new_secret_id("msg"),
                text: text.into(),
                attaches: None,
                reply,
                media: None,
                forwarded,
            },
            now_ms,
            Vec::new(),
        )?;
        self.update(Vec::new())
    }

    pub fn queue_annotation(
        &mut self,
        space: &str,
        target: &str,
        note: types::Annotation,
        now_ms: i64,
    ) -> Result<Update, String> {
        let target = self
            .resolve_post(space, target)
            .map(|event| event.hash.clone())
            .ok_or("That message is no longer available.")?;
        let author = self
            .logs
            .get(space)
            .and_then(|log| log.events().iter().find(|e| e.hash == target))
            .filter(|e| is_post(&e.body))
            .map(|e| e.author.clone())
            .ok_or("That message is no longer available.")?;
        if self.logs.get(space).is_some_and(|log| log.events().iter().any(|event| matches!(
            &event.body, EventBody::MessageAnnotated { target: to, note: types::Annotation::Delete }
                if self.resolve_post(space, to).is_some_and(|post| post.hash == target) && event.author == author
        ))) {
            return Err("That message is no longer available.".into());
        }
        if let types::Annotation::React { emoji } = &note {
            if emoji.trim().is_empty() || emoji.len() > 32 {
                return Err("Choose an emoji of up to 32 bytes.".into());
            }
        }
        if note.author_only() && author != self.account.identity {
            return Err("Only the person who sent this message can change it.".into());
        }
        // Deletion is terminal in the reader fold. Do not report a successful
        // author mutation that cannot take effect, including behind a queued delete.
        if note.author_only() {
            let deletes_target = |event: &Event| {
                event.author == author
                    && matches!(&event.body, EventBody::MessageAnnotated {
                        target: deleted, note: types::Annotation::Delete,
                    } if self.resolve_post(space, deleted).is_some_and(|post| post.hash == target))
            };
            let deleted = self
                .logs
                .get(space)
                .is_some_and(|log| log.events().iter().any(deletes_target))
                || self
                    .saved
                    .outbox
                    .values()
                    .filter(|pending| pending.failure.is_none())
                    .any(|pending| {
                        pending
                            .inner
                            .as_ref()
                            .is_some_and(|event| event.space == space && deletes_target(event))
                    });
            if deleted {
                return Err("Essa mensagem foi apagada.".into());
            }
        }
        if let types::Annotation::Edit { text } = &note {
            if text.trim().is_empty() || text.len() > 32_768 {
                return Err("Write a message of up to 32 KB.".into());
            }
        }
        self.queue_domain_body(
            space,
            EventBody::MessageAnnotated { target, note },
            now_ms,
            Vec::new(),
        )?;
        self.update(Vec::new())
    }

    pub fn queue_join(
        &mut self,
        space: &str,
        role: Role,
        code: &str,
        now_ms: i64,
    ) -> Result<Update, String> {
        if !protocol::valid_invite_code(code) || space.is_empty() {
            return Err("Invalid invitation.".into());
        }
        if self.roster(space).contains_key(&self.account.identity) {
            return self.update(Vec::new());
        }
        let mut pending = self.clear_event(
            space,
            EventBody::MemberAdded {
                identity: self.account.identity.clone(),
                role,
            },
            now_ms,
        )?;
        pending.invite = Some(code.to_string());
        let id = pending.envelope()?.client_id().to_string();
        self.saved.outbox.insert(id, pending);
        self.update(Vec::new())
    }
}
