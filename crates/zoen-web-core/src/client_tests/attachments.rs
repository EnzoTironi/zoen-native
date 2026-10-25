//! Original whole test bodies, called by their unchanged registered facade.
use super::*;

pub(super) fn attachment_intent_persists_both_signed_events_and_real_ciphertext_before_any_publish(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let root = clear(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_root".into(),
            text: "Original root".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
        9,
    )?;
    client.receive(&root, 9).unwrap();
    let reference = log.events().last().unwrap().hash.clone();
    let sealed = crate::media::seal_file(
        "report.txt",
        "text/plain",
        b"private attachment bytes",
        Some(b"private thumbnail"),
    )
    .unwrap();
    let frame = attachment_frame(&sealed);
    let mut request = issued_attachment(&mut client, log.space());
    request["reply_to"] = serde_json::json!(reference);
    request["thread"] = serde_json::json!(true);
    let (staged, item, message) = client
        .queue_attachment(&request.to_string(), &frame, 10)
        .unwrap();
    assert!(!staged
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    assert_eq!(client.saved.outbox.len(), 2);
    assert!(client
        .saved
        .outbox
        .values()
        .all(|pending| pending.blob_uploads.len() == 2));
    assert_eq!(client.saved.local_chunks.len(), 2);
    // Decode an actual cold saved snapshot; no additional file/receipt journal exists.
    let saved: serde_json::Value = serde_json::from_str(&staged.snapshot).unwrap();
    assert!(saved.get("attachments").is_none());
    let mut restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &staged.snapshot,
    )
    .unwrap();
    assert!(
        restored
            .queue_attachment(&request.to_string(), &frame, 11)
            .is_err(),
        "cold restore must refuse the old producer instead of duplicating its accepted send"
    );
    let before_upload = restored
        .receive(&ServerFrame::SyncDone.encode(), 12)
        .unwrap();
    assert!(!before_upload
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    for upload in &sealed.uploads {
        assert_eq!(restored.local_chunk(&upload.blob), upload.sealed);
    }
    let first = restored
        .document_uploaded(&[sealed.uploads[0].blob.clone()])
        .unwrap();
    assert!(!first
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    let ready = restored
        .document_uploaded(&[sealed.uploads[1].blob.clone()])
        .unwrap();
    assert_eq!(
        ready
            .frames
            .iter()
            .filter(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. })))
            .count(),
        2
    );
    confirm_documents(&mut restored, &mut log, ready, 13)?;
    assert!(restored.saved.outbox.is_empty());
    let conversations = restored.conversations();
    let posted = conversations[0]
        .messages
        .iter()
        .find(|entry| entry.id == message)
        .unwrap();
    assert_eq!(posted.text, request["caption"].as_str().unwrap());
    assert_eq!(posted.file_item_id.as_deref(), Some(item.as_str()));
    assert_eq!(posted.thread.as_deref(), Some(reference.as_str()));
    assert_eq!(posted.file.as_ref().unwrap(), &sealed.doc);
    assert_eq!(
        crate::media::open_chunk(
            &sealed.doc.chunks[0],
            &restored.local_chunk(&sealed.doc.chunks[0].blob)
        )
        .unwrap(),
        b"private attachment bytes"
    );
    Ok(())
}

pub(super) fn attachment_intent_lost_response_reencrypt_retry_and_ui_ack_have_one_canonical_effect(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, log) = document_fixture()?;
    let first =
        crate::media::seal_file("report.txt", "text/plain", b"one selection", None).unwrap();
    let request = issued_attachment(&mut client, log.space());
    let (_, item, message) = client
        .queue_attachment(&request.to_string(), &attachment_frame(&first), 10)
        .unwrap();
    let accepted = serde_json::to_string(&client.saved).unwrap();
    let fresh =
        crate::media::seal_file("report.txt", "text/plain", b"one selection", None).unwrap();
    assert_ne!(fresh.doc.chunks[0].blob, first.doc.chunks[0].blob);
    let (_, retried_item, retried_message) = client
        .queue_attachment(&request.to_string(), &attachment_frame(&fresh), 11)
        .unwrap();
    assert_eq!((retried_item, retried_message), (item, message));
    assert_eq!(
        serde_json::to_string(&client.saved).unwrap(),
        accepted,
        "a retry cannot replace accepted ciphertext or append another event"
    );
    let mut drift = request.clone();
    drift["caption"] = serde_json::json!("changed");
    assert!(client
        .queue_attachment(&drift.to_string(), &attachment_frame(&fresh), 12)
        .is_err());
    assert_eq!(serde_json::to_string(&client.saved).unwrap(), accepted);
    let ack = serde_json::json!({"op":"accepted","mutation_id":request["mutation_id"]}).to_string();
    client.attachment_producer(&ack).unwrap();
    client.attachment_producer(&ack).unwrap(); // lost ACK response is itself retryable
    assert!(
        client
            .queue_attachment(&request.to_string(), &attachment_frame(&fresh), 13)
            .is_err(),
        "ACK retires this sequence instead of reinterpreting it as a fresh Send"
    );
    assert_eq!(serde_json::to_string(&client.saved).unwrap(), accepted);
    Ok(())
}

pub(super) fn attachment_intent_gaps_drop_unissued_and_retired_leases_fail_closed_without_lifetime_cap(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, log) = document_fixture()?;
    let first = issued_attachment(&mut client, log.space());
    let second = issued_attachment(&mut client, log.space());
    let frame = attachment_frame(
        &crate::media::seal_file("report.txt", "text/plain", b"exact", None).unwrap(),
    );
    assert!(
        client
            .attachment_producer(
                &serde_json::json!({"op":"accepted","mutation_id":first["mutation_id"]})
                    .to_string()
            )
            .is_err(),
        "UI cannot acknowledge unaccepted input"
    );
    client
        .queue_attachment(&second.to_string(), &frame, 10)
        .unwrap();
    client
        .attachment_producer(
            &serde_json::json!({"op":"accepted","mutation_id":second["mutation_id"]}).to_string(),
        )
        .unwrap();
    client
        .queue_attachment(&first.to_string(), &frame, 11)
        .unwrap(); // second ACK cannot retire this pending gap
    client
        .attachment_producer(
            &serde_json::json!({"op":"drop","mutation_id":first["mutation_id"]}).to_string(),
        )
        .unwrap();
    assert!(client
        .queue_attachment(&first.to_string(), &frame, 12)
        .is_err());
    let before = serde_json::to_string(&client.saved).unwrap();
    for _ in 0..4096 {
        let request = issued_attachment(&mut client, log.space());
        client
            .attachment_producer(
                &serde_json::json!({"op":"drop","mutation_id":request["mutation_id"]}).to_string(),
            )
            .unwrap();
    }
    assert_eq!(
        serde_json::to_string(&client.saved).unwrap(),
        before,
        "issued/drop lifecycle has no durable lifetime index"
    );
    assert!(client
        .attachment_producer(r#"{"op":"issue","producer_id":"foreign"}"#)
        .is_err());
    let mut forged = first;
    forged["mutation_id"] = serde_json::json!("attachment_unknown_1");
    assert!(client
        .queue_attachment(&forged.to_string(), &frame, 13)
        .is_err());
    Ok(())
}

pub(super) fn attachment_intent_faults_preserve_existing_outbox_and_do_not_consume_issued_sequence(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, log) = document_fixture()?;
    let request = issued_attachment(&mut client, log.space());
    let frame = attachment_frame(
        &crate::media::seal_file("report.txt", "text/plain", b"exact", None).unwrap(),
    );
    let baseline = serde_json::to_string(&client.saved).unwrap();
    for bad in [
        vec![],
        frame[..3].to_vec(),
        {
            let mut bad = frame.clone();
            *bad.last_mut().unwrap() ^= 1;
            bad
        },
        {
            let mut bad = frame.clone();
            bad.push(1);
            bad
        },
    ] {
        assert!(client
            .queue_attachment(&request.to_string(), &bad, 10)
            .is_err());
        assert_eq!(serde_json::to_string(&client.saved).unwrap(), baseline);
    }
    let mut foreign = request.clone();
    foreign["reply_to"] = serde_json::json!("missing-message");
    foreign["thread"] = serde_json::json!(true);
    assert!(client
        .queue_attachment(&foreign.to_string(), &frame, 11)
        .is_err());
    assert_eq!(serde_json::to_string(&client.saved).unwrap(), baseline);
    client.saved.outbox_sequence = u64::MAX - 1;
    let full = serde_json::to_string(&client.saved).unwrap();
    assert!(client
        .queue_attachment(&request.to_string(), &frame, 12)
        .is_err());
    assert_eq!(
        serde_json::to_string(&client.saved).unwrap(),
        full,
        "two-event batch fails before either event or its ciphertext enters durable state"
    );
    client.saved.outbox_sequence = 0;
    client
        .queue_attachment(&request.to_string(), &frame, 13)
        .unwrap();
    Ok(())
}

pub(super) fn attachment_intent_authentic_chunk_length_lies_preserve_snapshot_and_issued_sequence(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, log) = document_fixture()?;
    let request = issued_attachment(&mut client, log.space());
    let baseline = client.update(Vec::new()).unwrap().snapshot;
    let mut sealed = crate::media::seal_file("report.txt", "text/plain", b"abc", None).unwrap();
    for claimed in [1, 4] {
        sealed.doc.bytes = claimed;
        sealed.doc.chunks[0].bytes = claimed;
        // These old checks authenticate the bytes/hash while accepting the false lengths.
        assert!(crate::media::validate_owned_file(&sealed.doc, 64 << 20).is_ok());
        assert_eq!(
            crate::media::open_chunk(&sealed.doc.chunks[0], &sealed.uploads[0].sealed).unwrap(),
            b"abc"
        );
        let error = client
            .queue_attachment(&request.to_string(), &attachment_frame(&sealed), 10)
            .err()
            .expect("authentic ciphertext with a false length must be refused");
        assert!(error.contains("chunk length"), "{error}");
        assert_eq!(client.update(Vec::new()).unwrap().snapshot, baseline);
        assert!(
            client
                .attachment_producer(
                    &serde_json::json!({
                        "op":"accepted", "mutation_id":request["mutation_id"]
                    })
                    .to_string()
                )
                .is_err(),
            "a rejected length cannot create an accepted receipt"
        );
    }
    let next = issued_attachment(&mut client, log.space());
    assert!(next["mutation_id"].as_str().unwrap().ends_with("_2"));
    sealed.doc.bytes = 3;
    sealed.doc.chunks[0].bytes = 3;
    client
        .queue_attachment(&request.to_string(), &attachment_frame(&sealed), 11)
        .unwrap();
    Ok(())
}

pub(super) fn attachment_intent_authentic_thumbnail_length_lies_preserve_snapshot_and_issued_sequence(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, log) = document_fixture()?;
    let request = issued_attachment(&mut client, log.space());
    let baseline = client.update(Vec::new()).unwrap().snapshot;
    let mut sealed =
        crate::media::seal_file("report.txt", "text/plain", b"abc", Some(b"thumb")).unwrap();
    for claimed in [1, 6] {
        sealed.doc.thumb.as_mut().unwrap().bytes = claimed;
        let thumb = sealed.doc.thumb.as_ref().unwrap();
        let upload = sealed
            .uploads
            .iter()
            .find(|upload| upload.blob == thumb.blob)
            .unwrap();
        assert!(crate::media::validate_owned_file(&sealed.doc, 64 << 20).is_ok());
        assert_eq!(
            crate::media::open_chunk(thumb, &upload.sealed).unwrap(),
            b"thumb"
        );
        let error = client
            .queue_attachment(&request.to_string(), &attachment_frame(&sealed), 10)
            .err()
            .expect("authentic thumbnail ciphertext with a false length must be refused");
        assert!(error.contains("thumbnail length"), "{error}");
        assert_eq!(client.update(Vec::new()).unwrap().snapshot, baseline);
        assert!(
            client
                .attachment_producer(
                    &serde_json::json!({
                        "op":"accepted", "mutation_id":request["mutation_id"]
                    })
                    .to_string()
                )
                .is_err(),
            "a rejected thumbnail cannot create an accepted receipt"
        );
    }
    let next = issued_attachment(&mut client, log.space());
    assert!(next["mutation_id"].as_str().unwrap().ends_with("_2"));
    sealed.doc.thumb.as_mut().unwrap().bytes = 5;
    client
        .queue_attachment(&request.to_string(), &attachment_frame(&sealed), 11)
        .unwrap();
    Ok(())
}
