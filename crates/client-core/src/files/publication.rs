//! Publication operations on the original Engine state.

use super::*;

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
        self.send_file_message_inner(
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
            None,
        )
    }

    /// Rust-only deterministic creation seam; the normal FileDoc/send transaction
    /// and receipt remain responsible for storage, attachment and exact retry.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn send_file_message_for_item(
        &mut self,
        space: &str,
        item: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
        reply_to: Option<&str>,
        mutation_id: &str,
    ) -> R<TimelineEntry> {
        self.send_file_message_inner(
            space,
            name,
            mime,
            bytes,
            None,
            reply_to,
            false,
            mutation_id,
            "",
            None,
            Some(item),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn send_file_message_inner(
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
        fixed_item: Option<&str>,
    ) -> R<TimelineEntry> {
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
        let reply = reply_to
            .map(|to| self.reply_ref(space, to, thread))
            .transpose()?;
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
                engine.file_add_at(space, name, name, mime, bytes, thumbnail, item)?
            } else {
                engine.file_add(space, name, name, mime, bytes, thumbnail)?
            };
            let posted = engine.append(
                space,
                &me,
                EventBody::MessagePosted {
                    message: new_id("msg"),
                    text: caption.to_string(),
                    attaches: Some(item),
                    reply,
                    media,
                    forwarded: false,
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
        let reply = reply_to
            .map(|to| self.reply_ref(space, to, thread))
            .transpose()?;
        self.create_voice_note(
            space,
            bytes,
            mime,
            VoiceMeta {
                ms,
                levels: VoiceMeta::encode(levels),
                transcript: transcript.trim().to_string(),
                reply,
                forwarded: false,
            },
        )
    }

    /// Copies one current verified file, freshly encrypted for its new audience.
    pub(crate) fn forward_file_message(
        &mut self,
        from_space: &str,
        item_id: &str,
        to_space: &str,
        caption: &str,
        voice: Option<&VoiceNoteDto>,
        media: Option<types::MessageMedia>,
    ) -> R<TimelineEntry> {
        let me = self.me_id()?;
        if !self.is_member(from_space, &me) || !self.state.writer(to_space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "encaminhamento de arquivo não autorizado",
                    "file forwarding is not authorized",
                ),
            });
        }
        let (item, file) = self.file_item(item_id)?;
        if item.space != from_space || file.is_voice() != voice.is_some() {
            return Err(invalid_file());
        }
        if voice.is_some_and(|voice| {
            voice.item_id != item_id || file.sha256 != voice.sha256 || file.mime != voice.mime
        }) {
            return Err(invalid_file());
        }
        let version = item.current().number;
        let bytes = self
            .file_bytes_bounded(item_id, Some(version), MAX_FILE_BYTES as u64)?
            .ok_or_else(|| {
                invalid(
                    "o arquivo ainda está chegando; tente novamente quando estiver disponível",
                    "the file is still arriving; try again when it is available",
                )
            })?;
        let thumbnail = match file.thumb.as_ref() {
            Some(chunk) => Some(
                self.file_chunk_bytes(chunk, MAX_THUMBNAIL_BYTES)?
                    .ok_or_else(|| {
                        invalid(
                            "a prévia do arquivo ainda está chegando; tente novamente quando estiver disponível",
                            "the file preview is still arriving; try again when it is available",
                        )
                    })?,
            ),
            None => None,
        };
        // Keys, chunks, upload queue and the signed file/message commit together.
        // On failure the shared atomic helper also restores the in-memory projection.
        self.atomic_agent_action(|engine| {
            if let Some(voice) = voice {
                return engine.create_voice_note(
                    to_space,
                    &bytes,
                    &file.mime,
                    VoiceMeta {
                        ms: voice.ms,
                        levels: VoiceMeta::encode(&voice.levels),
                        transcript: voice.transcript.clone(),
                        reply: None,
                        forwarded: true,
                    },
                );
            }
            // The destination starts at its own root. Treating a filename as a path
            // could turn ordinary audio named "VoiceNotes/…" into a legacy voice note.
            let item = engine.file_add(
                to_space,
                "",
                &file.name,
                &file.mime,
                &bytes,
                thumbnail.as_deref(),
            )?;
            let posted = engine.append(
                to_space,
                &me,
                EventBody::MessagePosted {
                    message: new_id("msg"),
                    text: caption.to_string(),
                    attaches: Some(item),
                    reply: None,
                    media,
                    forwarded: true,
                },
            )?;
            let message = if posted.client_id.is_empty() {
                posted.hash
            } else {
                posted.client_id
            };
            let entry = engine
                .space_state(to_space)?
                .entries
                .iter()
                .find(|entry| entry.id() == message)
                .ok_or_else(invalid_file)?;
            Ok(engine.entry_dto(entry))
        })
    }
}
