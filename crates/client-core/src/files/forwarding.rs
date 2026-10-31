//! The same atomic encrypted-file copier used by reviewed single/batch forwards.
use super::*;

pub(crate) struct ForwardFileInput<'a> {
    pub from_space: &'a str,
    pub item_id: &'a str,
    pub to_space: &'a str,
    pub caption: &'a str,
    pub voice: Option<&'a VoiceNoteDto>,
    pub media: Option<types::MessageMedia>,
    pub forwarded_author: Option<String>,
}
impl Engine {
    /// Copies one current verified file, freshly encrypted for its new audience.
    pub(crate) fn forward_file_message(&mut self, input: ForwardFileInput<'_>) -> R<TimelineEntry> {
        let ForwardFileInput {
            from_space,
            item_id,
            to_space,
            caption,
            voice,
            media,
            forwarded_author,
        } = input;
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
                        forwarded_author: forwarded_author.clone(),
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
                    message: new_id("msg")?,
                    text: caption.to_string(),
                    attaches: Some(item),
                    reply: None,
                    media,
                    forwarded: true,
                    forwarded_author: forwarded_author.clone(),
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
