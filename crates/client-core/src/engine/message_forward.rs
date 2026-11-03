//! Default and reviewed forwards share one signed copy writer.
use super::{not_found, t, CoreError, Engine, EntryBody, TimelineEntry, R};
use crate::forwarding::ForwardOptions;
use types::{new_id, EventBody, ItemContent};

impl Engine {
    /// Sends a copy of a message (text and attachment) to another chat, marked forwarded.
    pub fn forward(&mut self, from_space: &str, target: &str, to_space: &str) -> R<TimelineEntry> {
        self.forward_with_options(from_space, target, to_space, ForwardOptions::default())
    }

    pub(crate) fn forward_with_options(
        &mut self,
        from_space: &str,
        target: &str,
        to_space: &str,
        options: ForwardOptions,
    ) -> R<TimelineEntry> {
        let me = self.me_id()?;
        if !self.is_member(from_space, &me) || !self.state.writer(to_space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros encaminham mensagens para conversas em que podem escrever",
                    "only members can forward messages to chats they can write in",
                ),
            });
        }
        let source = self
            .space_state(from_space)?
            .entries
            .iter()
            .find(|entry| entry.id() == target)
            .ok_or_else(|| not_found(&t("Mensagem", "Message")))?;
        if !self.visible_message(source) {
            return Err(not_found(&t("Mensagem", "Message")));
        }
        let EntryBody::Message {
            text,
            attaches,
            marks,
            ..
        } = &source.body
        else {
            return Err(not_found(&t("Mensagem", "Message")));
        };
        if marks.deleted {
            return Err(not_found(&t("Mensagem", "Message")));
        }
        let forwarded = !options.hide_author;
        let forwarded_author = if forwarded {
            self.forwarded_author(source)
        } else {
            None
        };
        let media = marks.media.as_ref().map(types::MessageMedia::without_group);
        let voice = self.entry_voice(source);
        let original_text = text.clone();
        let caption_file = attaches.as_ref().is_some_and(|item| self.state.items.get(item).is_some_and(|item| matches!(&item.current().content, ItemContent::File(file) if !file.is_voice())));
        let (text, attaches) = (
            if options.hide_caption && caption_file {
                String::new()
            } else {
                text.clone()
            },
            attaches.clone(),
        );
        // Resolve the canonical entry, including old Android markers. Its current
        // audio and metadata are copied; source item/reply/thread ids never travel.
        if from_space != to_space || (options.hide_author && voice.is_some()) {
            if let Some(voice) = voice {
                return self.forward_file_message(crate::files::ForwardFileInput {
                    from_space,
                    item_id: &voice.item_id,
                    to_space,
                    caption: "",
                    voice: Some(&voice),
                    media: None,
                    forwarded_author,
                    forwarded,
                });
            }
            if original_text.starts_with("⟦voice:") {
                return Err(CoreError::Invalid {
                    reason: t(
                        "esta mensagem de voz não está mais disponível",
                        "this voice message is no longer available",
                    ),
                });
            }
            if let Some(item) = attaches.as_deref() {
                if self.state.items.get(item).is_some_and(|item| item.space==from_space && matches!(&item.current().content,ItemContent::App(app) if app.app=="excalidraw")) {
                    return self.forward_app_document(from_space,item,to_space,&text,forwarded,forwarded_author);
                }
                if !self.state.items.get(item).is_some_and(|item| {
                    item.space == from_space
                        && matches!(&item.current().content, ItemContent::File(_))
                }) {
                    return Err(CoreError::Invalid {
                        reason: t(
                            "este item ainda não pode ser encaminhado a outra conversa",
                            "this item cannot be forwarded to another chat yet",
                        ),
                    });
                }
                return self.forward_file_message(crate::files::ForwardFileInput {
                    from_space,
                    item_id: item,
                    to_space,
                    caption: &text,
                    voice: None,
                    media,
                    forwarded_author,
                    forwarded,
                });
            }
        }
        let appended = self.append(
            to_space,
            &me,
            EventBody::MessagePosted {
                message: new_id("msg")?,
                text,
                attaches,
                reply: None,
                media,
                forwarded,
                forwarded_author,
            },
        )?;
        let s = self.space_state(to_space)?;
        let entry = s
            .entries
            .iter()
            .find(|entry| entry.id() == appended.client_id)
            .ok_or_else(|| CoreError::Storage {
                message: "The appended entry is missing from its projection.".into(),
            })?;
        Ok(self.entry_dto(entry))
    }
}
