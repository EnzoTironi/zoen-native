//! reuse cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn selection_revalidates_deletion_membership_version_hash_and_account_without_writes() {
    for change in [
        "delete",
        "membership",
        "version",
        "hash",
        "account",
        "bytes",
        "name",
        "mime",
    ] {
        let f = fixture();
        let (item, message) = add(&f, "application/pdf", "source.pdf", false).unwrap();
        confirm(&f);
        let mut source = selected(&f);
        let mut asset = input(&source, "copied");
        let mut scope = f.scope.clone();
        match change {
            "delete" => f.core.delete_message(f.source.clone(), message).unwrap(),
            "membership" => receive(
                &f,
                &f.scope.expected_identity_id,
                EventBody::MemberRemoved {
                    identity: f.scope.expected_identity_id.clone(),
                },
            )
            .unwrap(),
            "version" => f
                .core
                .lock()
                .file_new_version_typed(
                    &item,
                    b"new",
                    None,
                    "changed",
                    "source.pdf",
                    "application/pdf",
                )
                .unwrap(),
            "hash" => source.reference.sha256 = "0".repeat(64),
            "account" => scope.expected_device_id.push('x'),
            "bytes" => asset.bytes = b"different".to_vec(),
            "name" => asset.name = "other.pdf".into(),
            "mime" => asset.mime = "image/png".into(),
            _ => unreachable!(),
        }
        let before = counts(&f);
        assert!(
            f.core
                .save_media_draft_with_recent_sources(
                    scope,
                    "0".into(),
                    content("copied"),
                    vec![asset],
                    vec![binding(&source, "copied")]
                )
                .is_err(),
            "{change}"
        );
        assert_eq!(counts(&f), before, "{change} wrote durable state");
        assert_eq!(
            f.core.media_draft(f.scope.clone()).unwrap().content_json,
            "null"
        );
    }
}

#[test]
pub(super) fn accepted_copy_has_exact_retry_receipt_and_fresh_destination_file_and_keys() {
    let f = fixture();
    let (item, message) = add(&f, "application/pdf", "source.pdf", false).unwrap();
    confirm(&f);
    let source = selected(&f);
    let save = || {
        f.core.save_media_draft_with_recent_sources(
            f.scope.clone(),
            "0".into(),
            content("copied"),
            vec![input(&source, "copied")],
            vec![binding(&source, "copied")],
        )
    };
    let accepted = save().unwrap();
    f.core.delete_message(f.source.clone(), message).unwrap();
    assert_eq!(
        save().unwrap(),
        accepted,
        "lost ACK replays the already-authorized copy"
    );
    let sent = f
        .core
        .send_media_draft(f.scope.clone(), accepted.draft.revision)
        .unwrap();
    let crate::EntryKind::Message {
        card: Some(card), ..
    } = &sent.entries[0].kind
    else {
        panic!("missing copied file")
    };
    assert_ne!(card.item_id, item);
    let core = f.core.lock();
    let ItemContent::File(old) = &core.state.items[&item].current().content else {
        unreachable!()
    };
    let ItemContent::File(new) = &core.state.items[&card.item_id].current().content else {
        unreachable!()
    };
    assert_eq!(
        (new.mime.as_str(), new.sha256.as_str()),
        (old.mime.as_str(), old.sha256.as_str())
    );
    assert_ne!(new.chunks[0].key, old.chunks[0].key);
}

#[test]
pub(super) fn unconfirmed_and_recovery_only_sources_never_become_publishable_recent_files() {
    let f = fixture();
    let (item, message) = add(&f, "application/pdf", "refused.pdf", true).unwrap();
    assert!(list(&f, RecentAttachmentKind::File).items.is_empty());
    receive(
        &f,
        &f.scope.expected_identity_id,
        EventBody::MemberRemoved {
            identity: f.scope.expected_identity_id.clone(),
        },
    )
    .unwrap();
    let pending = f.core.lock().store.outbox().unwrap();
    for row in pending {
        assert!(f
            .core
            .lock()
            .reject(&row.event.client_id, "permission denied", true, None)
            .user_failed());
    }
    receive(
        &f,
        &f.scope.expected_identity_id,
        EventBody::MemberAdded {
            identity: f.scope.expected_identity_id.clone(),
            role: Role::Member,
        },
    )
    .unwrap();
    assert!(f.core.lock().state.is_recovery_item(&item));
    assert!(list(&f, RecentAttachmentKind::File).items.is_empty());
    let file = f.core.file_info_at(item.clone(), Some(1)).unwrap();
    let reference = RecentAttachmentReference {
        space_id: f.source.clone(),
        message_id: message,
        item_id: item,
        version: 1,
        sha256: file.sha256,
    };
    let before = counts(&f);
    assert!(f
        .core
        .save_media_draft_with_recent_sources(
            f.scope.clone(),
            "0".into(),
            content("copied"),
            vec![MediaDraftAssetInput {
                id: "copied".into(),
                name: "refused.pdf".into(),
                mime: "application/pdf".into(),
                bytes: b"refused.pdf".to_vec()
            }],
            vec![RecentAttachmentSourceBinding {
                asset_id: "copied".into(),
                kind: RecentAttachmentKind::File,
                reference
            }],
        )
        .is_err());
    assert_eq!(counts(&f), before);
}

#[test]
pub(super) fn all_source_bindings_are_validated_before_any_member_is_staged() {
    let f = fixture();
    add(&f, "application/pdf", "first.pdf", false).unwrap();
    add(&f, "application/pdf", "second.pdf", false).unwrap();
    confirm(&f);
    let rows = list(&f, RecentAttachmentKind::File).items;
    let mut invalid_second = binding(&rows[1], "second");
    invalid_second.reference.version += 1;
    let json = serde_json::json!({"kind":"attachments","reply_to":null,"items":[
        {"asset_id":"first","caption":"","mutation_id":"first"},
        {"asset_id":"second","caption":"","mutation_id":"second"}
    ]})
    .to_string();
    let before = counts(&f);
    assert!(f
        .core
        .save_media_draft_with_recent_sources(
            f.scope.clone(),
            "0".into(),
            json,
            vec![input(&rows[0], "first"), input(&rows[1], "second")],
            vec![binding(&rows[0], "first"), invalid_second],
        )
        .is_err());
    assert_eq!(counts(&f), before);
    assert!(f
        .core
        .media_draft(f.scope.clone())
        .unwrap()
        .assets
        .is_empty());
    assert!(f
        .core
        .save_media_draft_with_recent_sources(
            f.scope.clone(),
            "0".into(),
            content("first"),
            vec![input(&rows[0], "first")],
            vec![binding(&rows[0], "first"), binding(&rows[0], "first")],
        )
        .is_err());
    assert_eq!(counts(&f), before);
}

#[tokio::test]
pub(super) async fn json_contract_keeps_metadata_bounded_and_rejects_unknown_or_forged_scope_fields(
) {
    let f = fixture();
    add(&f, "application/pdf", "source.pdf", false).unwrap();
    confirm(&f);
    let request = serde_json::json!({"op":"recent_attachments","scope":f.scope,"kind":"file","query":"","cursor":null,"limit":1});
    let output: serde_json::Value =
        serde_json::from_str(&f.core.client_request(request.to_string()).await.unwrap()).unwrap();
    assert_eq!(output["items"].as_array().unwrap().len(), 1);
    assert_eq!(output["items"][0]["name"], "source.pdf");
    assert_eq!(output["items"][0]["bytes"], "10");
    assert!(output["items"][0].get("uri").is_none());
    let reference = output["items"][0]["reference"].clone();
    let validate = serde_json::json!({"op":"attachment_reference_validate","scope":f.scope,"kind":"file","reference":reference});
    assert!(f.core.client_request(validate.to_string()).await.is_ok());
    let mut unknown = request.clone();
    unknown["unexpected"] = true.into();
    assert!(f.core.client_request(unknown.to_string()).await.is_err());
    let mut forged = request;
    forged["scope"]["expected_device_id"] = "another-device".into();
    assert!(f.core.client_request(forged.to_string()).await.is_err());
    let mut unknown_ref = validate;
    unknown_ref["reference"]["uri"] = "file:///private/forged".into();
    assert!(f
        .core
        .client_request(unknown_ref.to_string())
        .await
        .is_err());
}
