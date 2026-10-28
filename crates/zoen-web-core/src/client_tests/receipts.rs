//! Original whole test bodies, called by their unchanged registered facade.
use super::*;

pub(super) fn web_receipts_replay_after_saved_unsent_frames_and_each_reconnect(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _, log, _) = web_fixture()?;
    let space = log.space().to_owned();
    let last = log.next_seq() - 1;
    client.logs.insert(space.clone(), log);
    let first = client.mark_read(&space)?; // Simulate a durable save, then losing the outgoing frame.
    assert_eq!(receipt_marks(&first.frames)?, vec![("read", last)]);
    assert_eq!(
        receipt_marks(&client.mark_read(&space)?.frames)?,
        vec![("read", last)]
    );
    let mut restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &first.snapshot,
    )?;
    for _ in 0..2 {
        restored.bootstrap()?;
        let update = restored.receive(&ServerFrame::SyncDone.encode(), 100)?;
        assert_eq!(
            receipt_marks(&update.frames)?,
            vec![("delivered", last), ("read", last)]
        );
        assert_eq!(restored.delivered(), Vec::<Vec<u8>>::new());
    }
    Ok(())
}

pub(super) fn actual_public_receive_emits_delivery_and_mark_read_replays_after_serialized_snapshot_reload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Actual signed canonical Event ingestion, not direct logs/receipt-map writes.
    // This is the Rust browser adapter, not WebCrypto/IDB or browser runtime proof.
    let (mut client, author, mut log) = document_fixture()?;
    let space = log.space().to_string();
    let wire = clear(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_actual_receipt_ingest".into(),
            text: "genuine receive receipt".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
        50,
    )?;
    let seq = log.next_seq() - 1;
    let received = client.receive(&wire, 51)?;
    assert!(receipt_marks(&received.frames)?.contains(&("delivered", seq)));
    let read = client.mark_read(&space)?;
    assert_eq!(receipt_marks(&read.frames)?, vec![("read", seq)]);
    let mut restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &read.snapshot,
    )?;
    restored.bootstrap()?;
    let replay = restored.receive(&ServerFrame::SyncDone.encode(), 60)?;
    assert_eq!(
        receipt_marks(&replay.frames)?,
        vec![("delivered", seq), ("read", seq)]
    );
    assert_eq!(restored.delivered(), Vec::<Vec<u8>>::new());
    Ok(())
}

pub(super) fn actual_signed_removal_discards_new_receipts_and_rejoin_cannot_authorize_the_discarded_arrival(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for remove_reader in [false, true] {
        let (mut client, owner, mut log) = document_fixture()?;
        let space = log.space().to_string();
        let peer = Author::device(
            &NativeSigner::from_secret(&[121; 32]),
            NativeSigner::from_secret(&[122; 32]),
        );
        let added = clear(
            &mut log,
            &owner,
            EventBody::MemberAdded {
                identity: peer.identity.clone(),
                role: Role::Owner,
            },
            60,
        )?;
        client.receive(&added, 61)?;
        let incoming = |kind| {
            ServerFrame::Ephemeral {
                space: space.clone(),
                from: peer.identity.clone(),
                kind,
            }
            .encode()
        };
        client.receive(&incoming(protocol::EphemeralKind::Read { seq: 7 }), 62)?;
        assert_eq!(client.saved.receipts[&space][&peer.identity], (7, 7));
        let subject = if remove_reader {
            owner.identity.clone()
        } else {
            peer.identity.clone()
        };
        let active_owner = if remove_reader { &peer } else { &owner };
        let removed = clear(
            &mut log,
            active_owner,
            EventBody::MemberRemoved {
                identity: subject.clone(),
            },
            63,
        )?;
        client.receive(&removed, 64)?;
        for kind in [
            protocol::EphemeralKind::Delivered { seq: u64::MAX },
            protocol::EphemeralKind::Read { seq: u64::MAX },
        ] {
            let update = client.receive(&incoming(kind), 65)?;
            assert_eq!(client.saved.receipts[&space][&peer.identity], (7, 7));
            assert!(client.deferred_receipts.is_empty());
            // Historical authorized marks persist through serialized reload.
            let restored = Client::new(
                client.account.clone(),
                client.signer.clone(),
                &update.snapshot,
            )?;
            assert_eq!(restored.saved.receipts[&space][&peer.identity], (7, 7));
        }
        let rejoined = clear(
            &mut log,
            active_owner,
            EventBody::MemberAdded {
                identity: subject,
                role: Role::Owner,
            },
            66,
        )?;
        client.receive(&rejoined, 67)?;
        assert_eq!(client.saved.receipts[&space][&peer.identity], (7, 7));
        client.receive(&incoming(protocol::EphemeralKind::Read { seq: 11 }), 68)?;
        assert_eq!(client.saved.receipts[&space][&peer.identity], (11, 11));
        client.receive(&incoming(protocol::EphemeralKind::Delivered { seq: 1 }), 69)?;
        assert_eq!(client.saved.receipts[&space][&peer.identity], (11, 11));
    }
    Ok(())
}

pub(super) fn actual_early_receipt_waits_for_confirmed_signed_membership_and_connection_buffer_is_bounded(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, owner, mut log) = document_fixture()?;
    let space = log.space().to_string();
    let peer = NativeSigner::from_secret(&[123; 32]).id();
    let early = ServerFrame::Ephemeral {
        space: space.clone(),
        from: peer.clone(),
        kind: protocol::EphemeralKind::Read { seq: 25 },
    }
    .encode();
    let update = client.receive(&early, 70)?;
    assert!(!client
        .saved
        .receipts
        .get(&space)
        .is_some_and(|rows| rows.contains_key(&peer)));
    assert_eq!(client.deferred_receipts.len(), 1);
    let restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )?;
    assert!(
        restored.deferred_receipts.is_empty(),
        "Unvalidated pairs never enter durable snapshot"
    );
    let added = clear(
        &mut log,
        &owner,
        EventBody::MemberAdded {
            identity: peer.clone(),
            role: Role::Member,
        },
        71,
    )?;
    client.receive(&added, 72)?;
    assert_eq!(client.saved.receipts[&space][&peer], (25, 25));
    assert!(client.deferred_receipts.is_empty());
    for n in 0..1024 {
        client.receive(
            &ServerFrame::Ephemeral {
                space: space.clone(),
                from: format!("{n:064x}"),
                kind: protocol::EphemeralKind::Delivered { seq: 1 },
            }
            .encode(),
            73,
        )?;
    }
    let overflow = ServerFrame::Ephemeral {
        space: space.clone(),
        from: "ff".repeat(32),
        kind: protocol::EphemeralKind::Read { seq: 2 },
    }
    .encode();
    assert_eq!(
        client
            .receive(&overflow, 74)
            .err()
            .ok_or("missing authenticated test err value")?,
        "receipt catch-up buffer exceeded"
    );
    client.bootstrap()?;
    assert!(client.deferred_receipts.is_empty());
    assert_eq!(client.saved.receipts[&space][&peer], (25, 25));
    Ok(())
}

pub(super) fn browser_closed_chat_posts_queue_offline_and_never_publish_plaintext_after_encryption(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    client.synced = false;
    let queued = client.queue_message(log.space(), "Uma mensagem offline", 10)?;
    assert_eq!(
        client.conversations()[0]
            .messages
            .last()
            .ok_or("missing authenticated test last value")?
            .text,
        "Uma mensagem offline"
    );
    assert!(!queued.frames.iter().any(|frame| matches!(
        ClientFrame::decode(frame).map_err(|error| error.to_string()),
        Ok(ClientFrame::Publish { .. })
    )));
    let upgraded = clear(&mut log, &author, EventBody::SpaceEncrypted, 11)?;
    client.receive(&upgraded, 12)?;
    let resumed = client.receive(&ServerFrame::SyncDone.encode(), 13)?;
    assert!(!resumed.frames.iter().any(|frame|matches!(ClientFrame::decode(frame).map_err(|error| error.to_string()),Ok(ClientFrame::Publish{env}) if env.plain_event().is_some_and(|event|matches!(event.body,EventBody::MessagePosted{..})))));
    Ok(())
}

pub(super) fn browser_file_cards_keep_the_native_message_identity_for_deletion_and_replies(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let file = crate::media::seal_file("report.txt", "text/plain", b"report", None)?.doc;
    let (update, item, message) = client.queue_file_record(log.space(), file, 10)?;
    let before = client.conversations();
    assert_eq!(before[0].messages.len(), 1);
    assert_eq!(before[0].messages[0].id, message);
    assert_eq!(
        before[0].messages[0].file_item_id.as_deref(),
        Some(item.as_str())
    );
    confirm_documents(&mut client, &mut log, update, 11)?;
    let posted = client.conversations()[0].messages[0]
        .entry
        .clone()
        .ok_or("missing authenticated test cloned value")?;
    let deleted = client.queue_annotation(log.space(), &posted, types::Annotation::Delete, 12)?;
    confirm_documents(&mut client, &mut log, deleted, 13)?;
    let after = client.conversations();
    assert!(after[0].messages[0].deleted);
    assert!(after[0].messages[0].file.is_none());
    Ok(())
}
