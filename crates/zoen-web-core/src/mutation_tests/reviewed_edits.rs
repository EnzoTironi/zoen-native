use super::*;
use protocol::message_edit::MessageEdit;
fn review(client: &Client<HostSigner>, target: &str) -> MessageEdit {
    let source = client
        .conversations()
        .into_iter()
        .flat_map(|c| c.messages)
        .find(|m| m.entry.as_deref() == Some(target))
        .expect("confirmed source");
    MessageEdit {
        expected_identity_id: client.account.identity.clone(),
        expected_device_id: client.account.device.clone(),
        space_id: SPACE.into(),
        message_id: source.id,
        expected_revision: source.text_revision.expect("revision"),
        text: "Reviewed browser edit".into(),
    }
}
#[test]
fn reviewed_edit_keeps_pending_and_confirmed_retries_single_across_restore() {
    fixture(|client, _, _, log| {
        let update = client.queue_message(SPACE, "Original", 10).expect("post");
        let target = accept(client, log, update, 11);
        let input = review(client, &target);
        let update = client
            .queue_reviewed_edit(input.clone(), 12)
            .expect("reviewed edit");
        let pending = client.saved.outbox.len();
        client
            .queue_reviewed_edit(input.clone(), 13)
            .expect("pending retry");
        assert_eq!(client.saved.outbox.len(), pending);
        accept(client, log, update, 14);
        let source = client.conversations().remove(0).messages.remove(0);
        assert_eq!(source.text, input.text);
        assert_ne!(
            source.text_revision.as_deref(),
            Some(input.expected_revision.as_str())
        );
        let before = client.saved.outbox.len();
        reload(client);
        client
            .queue_reviewed_edit(input, 15)
            .expect("confirmed cold retry");
        assert_eq!(client.saved.outbox.len(), before);
    });
}
#[test]
fn old_revision_or_wrong_owner_cannot_queue_an_edit() {
    fixture(|client, _, _, log| {
        let update = client.queue_message(SPACE, "Original", 10).expect("post");
        let target = accept(client, log, update, 11);
        let input = review(client, &target);
        let update = client
            .queue_annotation(
                SPACE,
                &target,
                Annotation::Edit {
                    text: "Newer content".into(),
                },
                12,
            )
            .expect("newer edit");
        accept(client, log, update, 13);
        let before = client.saved.outbox.len();
        assert!(client.queue_reviewed_edit(input.clone(), 14).is_err());
        for device in [false, true] {
            let mut invalid = review(client, &target);
            if device {
                invalid.expected_device_id.push('x');
            } else {
                invalid.expected_identity_id.push('x');
            }
            assert!(client.queue_reviewed_edit(invalid, 15).is_err());
        }
        assert_eq!(client.saved.outbox.len(), before);
        assert_eq!(client.conversations()[0].messages[0].text, "Newer content");
    });
}
#[test]
fn pending_and_confirmed_deletion_remain_terminal_for_reviewed_edits() {
    fixture(|client, _, _, log| {
        let update = client.queue_message(SPACE, "Original", 10).expect("post");
        let target = accept(client, log, update, 11);
        let mut input = review(client, &target);
        input.text = "Original".into();
        let update = client
            .queue_annotation(SPACE, &target, Annotation::Delete, 12)
            .expect("delete");
        let before = client.saved.outbox.len();
        assert!(client.queue_reviewed_edit(input.clone(), 13).is_err());
        assert_eq!(client.saved.outbox.len(), before);
        accept(client, log, update, 14);
        reload(client);
        let before = client.saved.outbox.len();
        assert!(client.queue_reviewed_edit(input, 15).is_err());
        assert_eq!(client.saved.outbox.len(), before);
    });
}

#[test]
fn a_valid_member_cannot_edit_a_received_message_with_their_own_review_scope() {
    fixture(|client, native, author, log| {
        let target = peer_event(
            client,
            native,
            author,
            log,
            EventBody::MessagePosted {
                message: "peer_owned".into(),
                text: "Received original".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
            10,
        )
        .expect("peer's genuine encrypted source");
        let input = review(client, &target);
        let before = client.saved.outbox.len();
        assert!(client.queue_reviewed_edit(input, 12).is_err());
        assert_eq!(client.saved.outbox.len(), before);
        assert_eq!(
            client.conversations()[0].messages[0].text,
            "Received original"
        );
    });
}
