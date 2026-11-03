//! Reviewed private context crosses the real relay with independently enrolled peers.
use super::*;
use client_core::{MediaDraftAssetInput, MediaDraftScope, PrivacyDto};

fn audio() -> Vec<u8> {
    let samples = vec![0u8; 16_000];
    let size = u32::try_from(samples.len()).expect("bounded fixture");
    let mut wav = b"RIFF".to_vec();
    wav.extend((size + 36).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(8_000u32.to_le_bytes());
    wav.extend(16_000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(size.to_le_bytes());
    wav.extend(samples);
    wav
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reviewed_media_replies_recover_cold_deliver_once_and_never_grant_source_history(
) -> TestResult {
    let (w, space) = enrolled("media_reply_peers").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let private =
        alice
            .0
            .create_group_with("Private quote source".into(), vec![], PrivacyDto::Closed)?;
    let source = alice
        .0
        .send_message(private.clone(), "Alpha 😀 Beta".into())?;
    let local = bob.0.send_message(space.clone(), "Alpha 😀 Beta".into())?;
    received(&alice.0, &space, |rows| {
        rows.iter().any(|e| e.id == local.id)
    })
    .await?;
    alice.0.wait_until_settled(20_000).await?;
    drop(alice);
    for (voice, shared) in [(false, false), (false, true), (true, false), (true, true)] {
        let alice = Open::offline(&w, "alice")?;
        let account = alice.0.account().ok_or("Alice account")?;
        let scope = MediaDraftScope {
            expected_identity_id: account.identity_id.clone(),
            expected_device_id: account.device_id.clone(),
            space_id: space.clone(),
            thread_root_id: None,
        };
        let quoted = if shared { &source } else { &local };
        let selection = serde_json::json!({"source_id":quoted.id,"revision":quoted.text_revision,"text":"😀","spans":[{"start_utf16":6,"end_utf16":8}]});
        let mut value = if voice {
            serde_json::json!({"kind":"voice","reply_to":null,"source_asset_id":"original","editor":null,
                "render":{"asset_id":"render","mime":"audio/wav","duration_ms":1000,"levels":[0.1,0.8],"transcript":"Spoken result 雨"}})
        } else {
            serde_json::json!({"kind":"attachments","reply_to":null,"items":[{"asset_id":"original","mutation_id":format!("file_quote_{shared}"),"caption":"Exact caption 雨"}]})
        };
        if shared {
            let review = serde_json::json!({"op":"review_shared_reply","expected_identity_id":account.identity_id,
                "expected_device_id":account.device_id,"space_id":space,"thread_root_id":null,
                "source_space_id":private,"message_id":source.id,"expected_source_revision":source.text_revision,"selection":selection});
            value["shared_reply"] =
                serde_json::from_str(&alice.0.client_request(review.to_string()).await?)?;
        } else {
            value["reply_to"] = local.id.clone().into();
            value["reply_quote"] = selection;
        }
        value["reply_intent"] = format!("private_intent_{voice}_{shared}").into();
        let bytes = if voice {
            audio()
        } else {
            b"Reviewed original document bytes".to_vec()
        };
        let inputs = if voice {
            vec!["original", "render"]
        } else {
            vec!["original"]
        }
        .into_iter()
        .map(|id| MediaDraftAssetInput {
            id: id.into(),
            name: if voice {
                format!("{id}.wav")
            } else {
                "report.bin".into()
            },
            mime: if voice {
                "audio/wav".into()
            } else {
                "application/octet-stream".into()
            },
            bytes: bytes.clone(),
        })
        .collect();
        let initial = alice.0.media_draft(scope.clone())?;
        let before = alice.0.log_events(space.clone())?;
        let saved = alice
            .0
            .save_media_draft(scope.clone(), initial.revision, value.to_string(), inputs)?
            .draft;
        assert_eq!(
            alice.0.log_events(space.clone())?,
            before,
            "private staging cannot publish"
        );
        drop(alice);
        let alice = Open::offline(&w, "alice")?;
        assert_eq!(alice.0.media_draft(scope.clone())?, saved);
        let sent = alice
            .0
            .send_media_draft(scope.clone(), saved.revision.clone())?;
        assert_eq!(sent.entries.len(), 1);
        let id = sent.entries[0].id.clone();
        let accepted_events = alice.0.log_events(space.clone())?;
        drop(alice);
        let alice = Open::offline(&w, "alice")?;
        let replay = alice
            .0
            .send_media_draft(scope.clone(), saved.revision.clone())?;
        assert_eq!(replay.entries[0].id, id);
        assert_eq!(alice.0.log_events(space.clone())?, accepted_events);
        alice.0.start_sync(None)?;
        alice.0.wait_until_settled(20_000).await?;
        let settled = alice.0.log_events(space.clone())?;
        assert_eq!(
            alice.0.send_media_draft(scope, saved.revision)?.entries[0].id,
            id
        );
        assert_eq!(alice.0.log_events(space.clone())?, settled);
        let rows = received(&bob.0, &space, |rows| rows.iter().any(|e| e.id == id)).await?;
        assert_eq!(rows.iter().filter(|e| e.id == id).count(), 1);
        let delivered = find(&rows, &id)?;
        assert!(!serde_json::to_string(&delivered)?.contains("private_intent_"));
        let reply = delivered.reply_to.as_ref().ok_or("delivered quote")?;
        assert_eq!(reply.id, quoted.id);
        assert_eq!(reply.text, "😀");
        assert_eq!(
            reply.selection.as_ref().ok_or("exact selection")?.text,
            "😀"
        );
        if shared {
            let origin = reply.origin.as_ref().ok_or("shared source")?;
            assert_eq!(origin.space_id, private);
            assert!(!origin.available);
            assert!(!bob.0.spaces().iter().any(|s| s.id == private));
            assert!(bob
                .0
                .client_request(
                    serde_json::json!({"op":"message","space_id":private,"message_id":source.id})
                        .to_string()
                )
                .await
                .is_err());
        } else {
            assert!(reply.origin.is_none());
        }
        let (item, version) = if voice {
            let v = delivered.voice.as_ref().ok_or("voice")?;
            assert_eq!(v.transcript, "Spoken result 雨");
            (v.item_id.clone(), 1)
        } else {
            let EntryKind::Message {
                text,
                card: Some(card),
            } = &delivered.kind
            else {
                return Err("file message".into());
            };
            assert_eq!(text, "Exact caption 雨");
            (card.item_id.clone(), card.version)
        };
        bob.0
            .ensure_file_version(item.clone(), version, 65_536, 20_000)
            .await?;
        assert_eq!(bob.0.file_bytes(item, Some(version))?, Some(bytes));
    }
    Ok(())
}
