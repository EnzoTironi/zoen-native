use super::*;
use types::{EventBody, Identity, Role};

pub(super) fn fixture() -> (std::sync::Arc<Client>, SiteScope, SiteSource) {
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    let mut e = client.lock();
    e.create_account("Owner", "site_owner", "https://relay.example.test")
        .unwrap();
    e.set_registered(true).unwrap();
    let peer = e
        .create_identity(
            Identity {
                id: String::new(),
                kind: IdentityKind::Person,
                name: "Peer".into(),
                handle: "site_peer".into(),
                tint_hex: "#000000".into(),
                glyph: None,
                owner: None,
                bio: String::new(),
                owner_proof: None,
            },
            true,
        )
        .unwrap();
    let space_id = e.start_direct(&peer).unwrap();
    let account = e.account().unwrap();
    let scope = SiteScope {
        identity_id: account.identity.clone(),
        device_id: account.device.clone(),
        space_id,
    };
    let source = e
        .site_create(
            &scope,
            "first.html",
            "<!doctype html><h1>Private</h1>",
            "create_site_1",
        )
        .unwrap();
    drop(e);
    (client, scope, source)
}
pub(super) fn review(
    scope: &SiteScope,
    source: &SiteSource,
) -> std::result::Result<SiteReview, Box<dyn std::error::Error>> {
    Ok(SiteReview {
        scope: scope.clone(),
        source: source.source.clone(),
        relay_url: "https://relay.example.test".into(),
        configuration: Configuration {
            schema: SCHEMA,
            audience: "relay.test".into(),
            management_origin: "https://api.example.test".into(),
            public_origin: "https://sites.example.test".into(),
            max_html_bytes: MAX_HTML_BYTES,
        },
        mutation: Mutation {
            schema: SCHEMA,
            public_origin: "https://sites.example.test".into(),
            site_id: protocol::sites::site_id(
                &scope.identity_id,
                &scope.space_id,
                &source.source.item_id,
                "https://sites.example.test",
            )
            .unwrap(),
            request_id: types::new_secret_id("pub")?,
            expected_head: None,
            change: Change::Publish {
                source: source.source.clone(),
            },
        },
    })
}

#[test]
fn create_retry_reuses_file_and_message_then_checked_save_preserves_selected_old_version() {
    let (c, scope, first) = fixture();
    let mut e = c.lock();
    let retry = e
        .site_create(
            &scope,
            "first.html",
            "<!doctype html><h1>Private</h1>",
            "create_site_1",
        )
        .unwrap();
    assert_eq!(first, retry);
    assert!(e
        .site_create(&scope, "first.html", "different", "create_site_1")
        .is_err());
    let second = e
        .site_save(&scope, &first.source, "<h1>Second</h1>")
        .unwrap();
    assert_eq!(second.source.version, first.source.version + 1);
    assert_eq!(second.html.as_deref(), Some("<h1>Second</h1>"));
    assert!(e
        .site_save(&scope, &first.source, "<h1>Overwrite</h1>")
        .is_err());
    let old = e
        .site_source(
            &scope,
            &first.source.item_id,
            first.source.version,
            &first.source.sha256,
        )
        .unwrap();
    assert_eq!(old.html, first.html);
    assert!(!old.editable);
    // Preparing the chosen saved version cannot read a transient editor draft.
    let signed = e
        .site_prepare(
            &review(&scope, &first).unwrap(),
            "https://relay.example.test",
            &review(&scope, &first).unwrap().configuration,
        )
        .unwrap();
    assert_eq!(signed.html, first.html);
    assert_ne!(signed.html, second.html);
}

#[test]
fn preparation_signs_exact_shared_numeric_contract_and_refuses_changed_content_or_destination() {
    let (c, scope, first) = fixture();
    let e = c.lock();
    let r = review(&scope, &first).unwrap();
    let prepared = e.site_prepare(&r, &r.relay_url, &r.configuration).unwrap();
    assert!(event_log::verify_sig(
        &scope.device_id,
        &prepared.signed.signing_bytes("relay.test").unwrap(),
        &prepared.signed.signature
    ));
    assert!(!event_log::verify_sig(
        &scope.device_id,
        &prepared.signed.signing_bytes("other").unwrap(),
        &prepared.signed.signature
    ));
    let value = serde_json::to_value(&prepared).unwrap();
    assert!(value["signed"]["at_ms"].is_i64());
    assert!(value["signed"]["mutation"]["change"]["source"]["byte_length"].is_u64());
    assert!(e
        .site_prepare(&r, "https://changed.test", &r.configuration)
        .is_err());
    assert!(e
        .site_prepare(
            &r,
            &r.relay_url,
            &Configuration {
                audience: "other".into(),
                ..r.configuration.clone()
            }
        )
        .is_err());
    let mut changed = r;
    let Change::Publish { source } = &mut changed.mutation.change else {
        unreachable!()
    };
    source.byte_length += 1;
    assert!(e
        .site_prepare(&changed, &changed.relay_url, &changed.configuration)
        .is_err());
}

#[test]
fn stale_device_wrong_space_reader_and_revoked_membership_do_not_read_edit_or_sign() {
    let (c, scope, first) = fixture();
    let mut e = c.lock();
    for changed in [
        SiteScope {
            device_id: "foreign".into(),
            ..scope.clone()
        },
        SiteScope {
            space_id: "foreign".into(),
            ..scope.clone()
        },
    ] {
        assert!(e
            .site_source(
                &changed,
                &first.source.item_id,
                first.source.version,
                &first.source.sha256
            )
            .is_err());
        assert!(e.site_save(&changed, &first.source, "<p>Bad</p>").is_err());
    }
    e.append(
        &scope.space_id,
        &scope.identity_id,
        EventBody::MemberAdded {
            identity: scope.identity_id.clone(),
            role: Role::Reader,
        },
    )
    .unwrap();
    let reader = e
        .site_source(
            &scope,
            &first.source.item_id,
            first.source.version,
            &first.source.sha256,
        )
        .unwrap();
    assert!(!reader.publish_allowed && !reader.editable);
    assert!(e.site_save(&scope, &first.source, "<p>Bad</p>").is_err());
    let r = review(&scope, &first).unwrap();
    assert!(e.site_prepare(&r, &r.relay_url, &r.configuration).is_err());
    e.append(
        &scope.space_id,
        &scope.identity_id,
        EventBody::MemberRemoved {
            identity: scope.identity_id.clone(),
        },
    )
    .unwrap();
    assert!(e
        .site_source(
            &scope,
            &first.source.item_id,
            first.source.version,
            &first.source.sha256
        )
        .is_err());
}

#[test]
fn device_revocation_invalid_html_and_hash_mismatch_fail_before_signing_or_writing() {
    let (c, scope, first) = fixture();
    let mut e = c.lock();
    for html in [
        String::new(),
        "<p>\0</p>".into(),
        "a".repeat(MAX_HTML_BYTES as usize + 1),
    ] {
        assert!(e.site_save(&scope, &first.source, &html).is_err());
    }
    assert!(e
        .site_source(
            &scope,
            &first.source.item_id,
            first.source.version,
            &"0".repeat(64)
        )
        .is_err());
    e.store
        .set_meta(
            "devices.revoked",
            &serde_json::to_string(&vec![&scope.device_id]).unwrap(),
        )
        .unwrap();
    assert!(e
        .site_source(
            &scope,
            &first.source.item_id,
            first.source.version,
            &first.source.sha256
        )
        .is_err());
    let r = review(&scope, &first).unwrap();
    assert!(e.site_prepare(&r, &r.relay_url, &r.configuration).is_err());
}

#[test]
fn unavailable_corrupt_non_utf8_and_non_html_sources_cannot_become_publication_bytes() {
    let (c, scope, first) = fixture();
    let mut e = c.lock();
    let chunk = match &e
        .item_state(&first.source.item_id)
        .unwrap()
        .current()
        .content
    {
        types::ItemContent::File(file) => file.chunks[0].sha256.clone(),
        _ => unreachable!(),
    };
    e.store
        .conn()
        .execute("DELETE FROM media WHERE sha256=?1", [&chunk])
        .unwrap();
    assert!(e
        .site_source(
            &scope,
            &first.source.item_id,
            first.source.version,
            &first.source.sha256
        )
        .unwrap()
        .html
        .is_none());
    let r = review(&scope, &first).unwrap();
    assert!(e.site_prepare(&r, &r.relay_url, &r.configuration).is_err());
    e.store
        .put_media(
            &chunk,
            "text/html",
            &vec![b'x'; first.source.byte_length as usize],
            0,
        )
        .unwrap();
    assert!(e
        .site_source(
            &scope,
            &first.source.item_id,
            first.source.version,
            &first.source.sha256
        )
        .is_err());
    for (mime, bytes) in [
        ("text/html", vec![0xff]),
        ("text/plain", b"<h1>Not HTML</h1>".to_vec()),
    ] {
        let item = e
            .file_add(&scope.space_id, "", "other.html", mime, &bytes, None)
            .unwrap();
        let file = e.file_info_at(&item, Some(1)).unwrap();
        assert!(e.site_source(&scope, &item, 1, &file.sha256).is_err());
    }
}

#[test]
fn unpublish_requires_an_observed_head_and_signs_no_html_payload() {
    let (c, scope, first) = fixture();
    let e = c.lock();
    let mut r = review(&scope, &first).unwrap();
    r.mutation.change = Change::Unpublish;
    assert!(e.site_prepare(&r, &r.relay_url, &r.configuration).is_err());
    r.mutation.expected_head = Some("a".repeat(64));
    let result = e.site_prepare(&r, &r.relay_url, &r.configuration).unwrap();
    assert!(result.html.is_none());
    assert!(event_log::verify_sig(
        &scope.device_id,
        &result
            .signed
            .signing_bytes(&r.configuration.audience)
            .unwrap(),
        &result.signed.signature
    ));
    let mut tampered = result.signed.clone();
    tampered.mutation.expected_head = Some("b".repeat(64));
    assert!(!event_log::verify_sig(
        &scope.device_id,
        &tampered.signing_bytes(&r.configuration.audience).unwrap(),
        &tampered.signature
    ));
}

#[tokio::test]
async fn offline_client_does_not_trust_cached_relay_name_or_emit_a_signed_preparation() {
    let (c, scope, first) = fixture();
    c.lock().store.set_meta("relay_name", "relay.test").unwrap();
    assert!(c
        .site_review(scope.clone(), first.source.clone(), false)
        .await
        .is_err());
    assert!(c.site_prepare(review(&scope, &first)).await.is_err());
}

#[tokio::test]
async fn real_json_commands_keep_signed_lengths_numeric_and_allow_only_bounded_html_writes() {
    let (c, scope, first) = fixture();
    let read = c
        .client_request(
            serde_json::json!({ "op": "site_read", "scope": scope,
        "item_id": first.source.item_id, "version": first.source.version,
        "expected_sha256": first.source.sha256 })
            .to_string(),
        )
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&read).unwrap();
    assert!(value["source"]["byte_length"].is_u64());
    assert_eq!(value["html"], first.html.unwrap());
    let html = "é".repeat(600_000);
    let request = serde_json::json!({ "op": "site_create", "scope": scope, "name": "large.html", "html": html, "mutation_id": "large_site" }).to_string();
    assert!(request.len() > crate::client_api::MAX_REQUEST_BYTES);
    let result = c.client_request(request).await.unwrap();
    let result: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["source"]["byte_length"], 1_200_000);
    let unrelated =
        serde_json::json!({ "op": "page_parse_markdown", "markdown": "x".repeat(1_200_000) })
            .to_string();
    assert!(c.client_request(unrelated).await.is_err());
}
