//! Publication on the original Engine and canonical document state.

use super::*;

impl Engine {
    pub(super) fn send_media_draft(
        &mut self,
        scope: &MediaDraftScope,
        expected: &str,
    ) -> R<MediaDraftSendResult> {
        let storage = self.media_draft_scope(scope)?;
        let mut record = self.load_media_draft(&storage)?;
        if !intent_id(expected) {
            return Err(stale());
        }
        if let Some(receipt) = record
            .last_accepted
            .as_ref()
            .filter(|receipt| receipt.input_revision == expected)
        {
            let timeline = self.timeline(&scope.space_id)?;
            let entries = receipt
                .messages
                .iter()
                .map(|id| {
                    timeline
                        .iter()
                        .find(|entry| &entry.id == id)
                        .cloned()
                        .ok_or_else(stale)
                })
                .collect::<R<Vec<_>>>()?;
            return Ok(MediaDraftSendResult {
                entries,
                draft: record.dto()?,
            });
        }
        if record.revision != expected || expected == MEDIA_DRAFT_REVISION_ZERO {
            return Err(stale());
        }
        let preflight = (|| -> R<()> {
            let content = record.content.as_ref().ok_or_else(invalid)?;
            if !self
                .state
                .writer(&scope.space_id, &scope.expected_identity_id)
            {
                return Err(CoreError::Forbidden {
                    reason: t("só autores enviam mídia", "only writers can send media"),
                });
            }
            let reply = match content {
                Content::Attachments { reply_to, .. } | Content::Voice { reply_to, .. } => {
                    reply_to.as_deref()
                }
            };
            self.draft_target(&scope.normal(), reply)?;
            if matches!(content, Content::Voice { render: None, .. }) {
                return Err(CoreError::Invalid {
                    reason: t(
                        "prepare a prévia do áudio antes de enviar",
                        "prepare the audio preview before sending",
                    ),
                });
            }
            Ok(())
        })();
        if let Err(error) = preflight {
            return Err(if record.attempted {
                error
            } else {
                CoreError::DraftRejected {
                    reason: error.to_string(),
                }
            });
        }
        // Authenticate every source/derived asset before admitting the immutable intent.
        let bytes = record
            .assets
            .iter()
            .map(|asset| self.load_media_asset(&storage, asset))
            .collect::<R<Vec<_>>>()?;
        if !record.attempted {
            record.attempted = true;
            self.atomic_agent_action(|engine| engine.persist_media_record(&storage, &record))?;
        }
        self.atomic_agent_action(|engine| {
            let reply = match record.content.as_ref().expect("preflight") {
                Content::Attachments { reply_to, .. } | Content::Voice { reply_to, .. } => {
                    reply_to.as_deref().or(scope.thread_root_id.as_deref())
                }
            };
            let thread = scope.thread_root_id.is_some();
            let entries = match record.content.as_ref().expect("preflight") {
                Content::Attachments { items, options, .. } => {
                    let mutations: Vec<&str> =
                        items.iter().map(|item| item.mutation_id.as_str()).collect();
                    let mut entries = Vec::with_capacity(items.len());
                    for (index, ((item, asset), bytes)) in
                        items.iter().zip(&record.assets).zip(&bytes).enumerate()
                    {
                        let media = options.metadata(
                            &scope.expected_identity_id,
                            &scope.space_id,
                            reply,
                            thread,
                            &mutations,
                            index,
                        );
                        entries.push(engine.send_file_message_with_media(
                            &scope.space_id,
                            &asset.name,
                            &asset.mime,
                            bytes,
                            None,
                            reply,
                            thread,
                            &item.mutation_id,
                            &item.caption,
                            media,
                        )?);
                    }
                    entries
                }
                Content::Voice {
                    render: Some(render),
                    ..
                } => vec![engine.send_voice_note(
                    &scope.space_id,
                    &bytes[1],
                    &render.mime,
                    render.duration_ms.round() as u64,
                    &render.levels,
                    &render.transcript,
                    reply,
                    thread,
                )?],
                _ => unreachable!("preflight"),
            };
            let cleared = Record {
                revision: types::new_id("media_draft"),
                content: None,
                assets: Vec::new(),
                attempted: false,
                last_accepted: Some(Receipt {
                    input_revision: expected.into(),
                    messages: entries.iter().map(|entry| entry.id.clone()).collect(),
                }),
                save_receipts: record.save_receipts.clone(),
            };
            for asset in &record.assets {
                engine.store.meta_delete(&storage.asset_key(&asset.id))?;
            }
            engine.persist_media_record(&storage, &cleared)?;
            Ok(MediaDraftSendResult {
                entries,
                draft: cleared.dto()?,
            })
        })
    }
}
