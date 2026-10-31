//! Publication operations on the original Engine state.

use super::{
    invalid, invalid_file, new_id, t, CoreError, Digest, Engine, EventBody, FileAddInput, Sha256,
    TimelineEntry, VoiceMeta, MAX_FILE_BYTES, MAX_FILE_CAPTION_BYTES, MAX_THUMBNAIL_BYTES, R,
};

/// Validated by the normal file-send boundary. A fixed ID is an optional Rust
/// creation fence; it grants no authority and cannot replace an existing Item.
pub(crate) struct FileSendInput<'a> {
    pub space: &'a str,
    pub name: &'a str,
    pub mime: &'a str,
    pub bytes: &'a [u8],
    pub thumbnail: Option<&'a [u8]>,
    pub reply_to: Option<&'a str>,
    pub thread: bool,
    pub mutation_id: &'a str,
    pub caption: &'a str,
    pub media: Option<types::MessageMedia>,
    pub fixed_item: Option<&'a str>,
    pub draft_reply: Option<crate::replies::external::DraftReplyContext<'a>>,
}

impl Engine {
    /// Stores an ordinary file and posts its attachment as one durable operation.
    /// A receipt binds retries to the original intent, including reply/thread scope.
    #[allow(clippy::too_many_arguments)]
    pub fn send_file_message(
        &mut self,
        space: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        reply_to: Option<&str>,
        thread: bool,
        mutation_id: &str,
    ) -> R<TimelineEntry> {
        self.send_file_message_with_caption(
            space,
            name,
            mime,
            bytes,
            thumbnail,
            reply_to,
            thread,
            mutation_id,
            "",
        )
    }

    /// The caption and file share one signed message and one retry receipt.
    #[allow(clippy::too_many_arguments)]
    pub fn send_file_message_with_caption(
        &mut self,
        space: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        reply_to: Option<&str>,
        thread: bool,
        mutation_id: &str,
        caption: &str,
    ) -> R<TimelineEntry> {
        self.send_file_message_with_media(
            space,
            name,
            mime,
            bytes,
            thumbnail,
            reply_to,
            thread,
            mutation_id,
            caption,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_file_message_with_media(
        &mut self,
        space: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        reply_to: Option<&str>,
        thread: bool,
        mutation_id: &str,
        caption: &str,
        media: Option<types::MessageMedia>,
    ) -> R<TimelineEntry> {
        self.send_file_message_inner(FileSendInput {
            space,
            name,
            mime,
            bytes,
            thumbnail,
            reply_to,
            thread,
            mutation_id,
            caption,
            media,
            fixed_item: None,
            draft_reply: None,
        })
    }

    pub(crate) fn send_file_message_inner(&mut self, input: FileSendInput<'_>) -> R<TimelineEntry> {
        let FileSendInput {
            space,
            name,
            mime,
            bytes,
            thumbnail,
            reply_to,
            thread,
            mutation_id,
            caption,
            media,
            fixed_item,
            draft_reply,
        } = input;
        let me = self.me_id()?;
        if !self.state.writer(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t("só autores enviam arquivos", "only writers can send files"),
            });
        }
        if mutation_id.is_empty()
            || mutation_id.len() > 128
            || !mutation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || name.trim().is_empty()
            || name.len() > 1024
            || name.contains(['/', '\\', '\0'])
            || mime.is_empty()
            || mime.len() > 255
            || !mime.contains('/')
            || mime.chars().any(char::is_control)
            || bytes.is_empty()
            || bytes.len() > MAX_FILE_BYTES
            || thumbnail.is_some_and(|t| t.is_empty() || t.len() > MAX_THUMBNAIL_BYTES)
            || caption.len() > MAX_FILE_CAPTION_BYTES
            || caption.contains('\0')
            || (thread && reply_to.is_none())
            || media.as_ref().is_some_and(|value| !value.is_valid())
        {
            return Err(invalid("envio de arquivo inválido", "invalid file send"));
        }
        // Resolve the target before file keys or chunks are written. Never fall back
        // to the main conversation when a thread target disappeared or is foreign.
        let reply = if let Some(context) = draft_reply {
            if context.scope.space_id != space
                || context.scope.thread_root_id.is_some() != thread
                || context.scope.expected_identity_id != me
            {
                return Err(invalid_file());
            }
            self.resolve_draft_reply(context, reply_to)?
        } else {
            reply_to
                .map(|to| self.reply_ref(space, to, thread))
                .transpose()?
        };
        let caption = if caption.trim().is_empty() {
            ""
        } else {
            caption
        };
        let file_intent = (
            space,
            name,
            mime,
            hex::encode(Sha256::digest(bytes)),
            thumbnail.map(|t| hex::encode(Sha256::digest(t))),
            reply_to,
            thread,
        );
        // Preserve the original blank-caption fingerprint for accepted sends
        // made before caption support. Nonempty captions cannot reuse it.
        let intent = if let Some(media) = media.as_ref() {
            serde_json::to_vec(&("zoen.media.send.v1", file_intent, caption, media))
        } else if caption.is_empty() {
            serde_json::to_vec(&file_intent)
        } else {
            serde_json::to_vec(&(file_intent, caption))
        }
        .map_err(|_| invalid_file())?;
        // New reviewed context is part of the immutable send. Preserve every
        // legacy fingerprint when no selected/shared quote is present.
        let intent = if reply
            .as_ref()
            .is_some_and(|r| r.quote.is_some() || r.shared.is_some())
        {
            serde_json::to_vec(&("zoen.file.send.reply.v1", intent, &reply))
                .map_err(|_| invalid_file())?
        } else {
            intent
        };
        let fingerprint = hex::encode(Sha256::digest(&intent));
        let key = format!("file_send:{me}:{mutation_id}");
        if let Some(receipt) = self.store.meta(&key)? {
            let (accepted, message): (String, String) =
                serde_json::from_str(&receipt).map_err(|_| invalid_file())?;
            if accepted != fingerprint {
                return Err(invalid(
                    "identificador de envio já utilizado",
                    "send identifier already used",
                ));
            }
            let entry = self
                .space_state(space)?
                .entries
                .iter()
                .find(|entry| entry.id() == message)
                .ok_or_else(invalid_file)?;
            let output = self.entry_dto(entry);
            if fixed_item.is_some_and(|item| !matches!(&output.kind, crate::EntryKind::Message {card:Some(card),..} if card.item_id==item)) {
                return Err(invalid_file());
            }
            return Ok(output);
        }
        self.atomic_agent_action(|engine| {
            let item = if let Some(item) = fixed_item {
                engine.file_add_at(
                    FileAddInput {
                        space,
                        path: name,
                        name,
                        mime,
                        bytes,
                        thumbnail,
                    },
                    item,
                )?
            } else {
                engine.file_add(space, name, name, mime, bytes, thumbnail)?
            };
            let posted = engine.append(
                space,
                &me,
                EventBody::MessagePosted {
                    message: new_id("msg")?,
                    text: caption.to_string(),
                    attaches: Some(item),
                    reply,
                    media,
                    forwarded: false,
                    forwarded_author: None,
                },
            )?;
            // Timeline identity is the signed event client ID, not MessagePosted.message.
            let message = if posted.client_id.is_empty() {
                posted.hash
            } else {
                posted.client_id
            };
            let receipt =
                serde_json::to_string(&(fingerprint, &message)).map_err(|_| invalid_file())?;
            engine.store.set_meta(&key, &receipt)?;
            let entry = engine
                .space_state(space)?
                .entries
                .iter()
                .find(|entry| entry.id() == message)
                .ok_or_else(invalid_file)?;
            Ok(engine.entry_dto(entry))
        })
    }

    /// Sends a voice note: the audio as an end-to-end encrypted file Item (the same chunked
    /// path as any file, so every client, the web included, can fetch and play it), tagged
    /// with its length, waveform and transcript. With `reply_to` it answers that message
    /// (inline, or in its thread).
    #[allow(clippy::too_many_arguments)]
    pub fn send_voice_note(
        &mut self,
        space: &str,
        bytes: &[u8],
        mime: &str,
        ms: u64,
        levels: &[f32],
        transcript: &str,
        reply_to: Option<&str>,
        thread: bool,
    ) -> R<TimelineEntry> {
        let reply = reply_to
            .map(|to| self.reply_ref(space, to, thread))
            .transpose()?;
        self.send_voice_note_with_meta(
            space,
            bytes,
            mime,
            VoiceMeta {
                ms,
                levels: VoiceMeta::encode(levels),
                transcript: transcript.trim().to_string(),
                reply,
                forwarded: false,
                forwarded_author: None,
            },
        )
    }

    pub(crate) fn send_voice_note_with_meta(
        &mut self,
        space: &str,
        bytes: &[u8],
        mime: &str,
        meta: VoiceMeta,
    ) -> R<TimelineEntry> {
        if !mime.starts_with("audio/") {
            return Err(invalid("não é um áudio", "not audio"));
        }
        let me = self.me_id()?;
        if !self.state.writer(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros mandam mensagens de voz",
                    "only members can send voice messages",
                ),
            });
        }
        self.create_voice_note(space, bytes, mime, meta)
    }
}
