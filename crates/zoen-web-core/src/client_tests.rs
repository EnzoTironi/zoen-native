use super::*;
use openmls_traits::{signatures::SignerError, types::SignatureScheme};
use roda_log::{Author, Signer as NativeSigner};
use roda_mls::Device;

#[derive(Clone)]
struct HostSigner(NativeSigner);
impl Signer for HostSigner {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        hex::decode(self.0.sign(payload)).map_err(|_| SignerError::SigningError)
    }
    fn signature_scheme(&self) -> SignatureScheme {
        roda_mls::SUITE.signature_algorithm()
    }
}

fn account(identity: &NativeSigner, device: &NativeSigner) -> LinkedAccount {
    LinkedAccount {
        identity: identity.id(),
        device: device.id(),
        cert: identity.sign(&roda_log::device_cert_message(&device.id())),
        name: "Fixture".into(),
        handle: "fixture".into(),
        relay: "http://127.0.0.1:9000".into(),
        primary: identity.id(),
        primary_hpke: String::new(),
        hpke_secret: String::new(),
        link_secret: String::new(),
        agreement_secret: String::new(),
        profile_key: String::new(),
        profile_key_version: 0,
        profile_seen_version: 0,
        profile_fields: String::new(),
    }
}

fn sequence(log: &mut SpaceLog, env: Envelope) -> Vec<u8> {
    let outer = roda_log::event_from_content(
        env.content().to_vec(),
        env.sig.clone(),
        env.cert.clone(),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    let event = log.sequence(outer);
    ServerFrame::Event {
        ev: Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env,
        },
    }
    .encode()
}

fn clear(log: &mut SpaceLog, author: &Author, body: EventBody, now: i64) -> Vec<u8> {
    let event = author.sign_event(
        log.space(),
        &roda_types::new_ulid(now),
        now,
        log.head(),
        body,
    );
    sequence(log, Envelope::plain(&event))
}

fn sealed(
    log: &mut SpaceLog,
    author: &Author,
    kind: SealedKind,
    data: Vec<u8>,
    now: i64,
) -> Vec<u8> {
    let (content, sig) = author.sign_sealed(
        log.space(),
        &roda_types::new_ulid(now),
        now,
        log.head().as_ref(),
        Sealed::new(kind, SUITE_ID, data),
    );
    sequence(
        log,
        Envelope::new(content, sig, author.cert.clone(), None).unwrap(),
    )
}

#[test]
fn native_sqlite_welcome_messages_saved_reload_and_removal_use_shared_mls() {
    let identity = NativeSigner::from_secret(&[11; 32]);
    let native_key = NativeSigner::from_secret(&[12; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[13; 32]));
    let account = account(&identity, &host.0);
    let author = Author::device(&identity, native_key.clone());
    let mut sqlite = rusqlite::Connection::open_in_memory().unwrap();
    roda_mls::migrate(&mut sqlite).unwrap();
    let native = Device::new(
        &sqlite,
        [14; 32],
        &identity.id(),
        native_key.secret(),
        author.cert.as_deref().unwrap(),
    )
    .unwrap();
    let mut client = Client::new(account.clone(), host.clone(), "").unwrap();
    let start = client.bootstrap().unwrap();
    let package = start
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                op: Op::PublishKeyPackages { packages, .. },
                ..
            } => packages.into_iter().next(),
            _ => None,
        })
        .unwrap();
    let space = "sp_native_web_fixture";
    let mut log = SpaceLog::new(space);
    let create = clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Native fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
        1,
    );
    client.receive(&create, 2).unwrap();
    assert!(client.queue_message(space, "not yet", 3).is_err());
    native.create_group(space).unwrap();
    let roster = BTreeSet::from([identity.id()]);
    let commit = native.commit(space, &[package], &BTreeSet::new()).unwrap();
    native
        .open(space, &commit.commit, &roster, native.leaf())
        .unwrap();
    client
        .receive(
            &sealed(&mut log, &author, SealedKind::Commit, commit.commit, 4),
            5,
        )
        .unwrap();
    client
        .receive(
            &sealed(
                &mut log,
                &author,
                SealedKind::Welcome,
                commit.welcome.unwrap(),
                6,
            ),
            7,
        )
        .unwrap();
    client.receive(&ServerFrame::SyncDone.encode(), 8).unwrap();
    assert_eq!(
        client.device().unwrap().checkpoint(space).unwrap(),
        native.checkpoint(space).unwrap()
    );
    assert!(client.conversations()[0].can_send);

    let inner = author.sign_event(
        space,
        &roda_types::new_ulid(9),
        9,
        log.head(),
        EventBody::MessagePosted {
            message: "msg_native".into(),
            text: "Native to web".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
    );
    let plaintext = InnerEvent {
        content: inner.content,
        sig: inner.sig,
    }
    .encode();
    let ciphertext = native.seal(space, &plaintext).unwrap();
    let (content, sig) = author.sign_sealed(
        space,
        &inner.client_id,
        9,
        inner.seen.as_ref(),
        Sealed::new(SealedKind::Application, SUITE_ID, ciphertext),
    );
    client
        .receive(
            &sequence(
                &mut log,
                Envelope::new(content, sig, author.cert.clone(), None).unwrap(),
            ),
            10,
        )
        .unwrap();
    assert_eq!(client.conversations()[0].messages[0].text, "Native to web");

    let sending = client.queue_message(space, "Web to native", 11).unwrap();
    let env = sending
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Publish { env }
                if env
                    .sealed_data()
                    .is_some_and(|(kind, _)| kind == SealedKind::Application) =>
            {
                Some(env)
            }
            _ => None,
        })
        .unwrap();
    env.verify().unwrap();
    let (_, ciphertext) = env.sealed_data().unwrap();
    let from = Leaf {
        identity: account.identity.clone(),
        device: account.device.clone(),
        cert: account.cert.clone(),
    };
    let Opened::Application { plaintext, .. } =
        native.open(space, ciphertext, &roster, &from).unwrap()
    else {
        panic!("real MLS application expected")
    };
    let opened = InnerEvent::decode(&plaintext).unwrap();
    let event = roda_log::event_from_content(
        opened.content,
        opened.sig,
        Some(account.cert.clone()),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    roda_log::verify_author(&event).unwrap();
    assert!(matches!(event.body, EventBody::MessagePosted { text, .. } if text == "Web to native"));
    let accepted = client.receive(&sequence(&mut log, env), 12).unwrap();
    assert_eq!(accepted.conversations[0].messages.len(), 2);
    assert_eq!(accepted.conversations[0].messages[1].delivery, "confirmed");

    let mut restored = Client::new(account, host, &accepted.snapshot).unwrap();
    restored.bootstrap().unwrap();
    restored
        .receive(&ServerFrame::SyncDone.encode(), 13)
        .unwrap();
    assert!(restored.conversations()[0].can_send);
    assert_eq!(restored.conversations()[0].messages.len(), 2);
    assert_eq!(
        restored.device().unwrap().checkpoint(space).unwrap(),
        native.checkpoint(space).unwrap()
    );
    restored
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MemberRemoved {
                    identity: identity.id(),
                },
                14,
            ),
            15,
        )
        .unwrap();
    assert!(restored.queue_message(space, "removed", 16).is_err());
    assert_eq!(
        restored.device().unwrap().group_state(space).unwrap(),
        GroupState::Missing
    );
}

#[path = "mutation_tests.rs"]
mod mutation_tests;

#[test]
fn invitation_queues_a_certified_self_add_with_the_original_code_and_no_plain_message_fallback() {
    let identity = NativeSigner::from_secret(&[21; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[22; 32]));
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
    assert!(client
        .queue_join("sp_invite", Role::Member, "invalid", 1)
        .is_err());
    let update = client
        .queue_join("sp_invite", Role::Member, "0123456789", 2)
        .unwrap();
    let env = update
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Publish { env } => Some(env),
            _ => None,
        })
        .unwrap();
    assert_eq!(env.invite.as_deref(), Some("0123456789"));
    env.verify().unwrap();
    assert!(
        matches!(env.plain_event().unwrap().body, EventBody::MemberAdded { identity: ref added, role: Role::Member } if added == &identity.id())
    );
    assert!(client
        .queue_message("sp_invite", "plain text forbidden", 3)
        .is_err());
}

#[test]
fn browser_internal_conversations_remain_in_detail_model() {
    let identity = NativeSigner::from_secret(&[71; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[72; 32]));
    let author = Author::device(&identity, host.0.clone());
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let mut root = SpaceLog::new("root");
    clear(
        &mut root,
        &author,
        EventBody::SpaceCreated {
            title: "Community".into(),
            kind: SpaceKind::Community,
            privacy: Privacy::Closed,
        },
        1,
    );
    clear(&mut root, &author, EventBody::MessagePosted { message: "register".into(), text: "zoen-living-space-meta-v1:{\"action\":\"conversation_partition\",\"child\":\"child\"}".into(), attaches: None, reply: None, forwarded: false }, 2);
    let mut child = SpaceLog::new("child");
    clear(
        &mut child,
        &author,
        EventBody::SpaceCreated {
            title: "Conversation".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
        1,
    );
    client.logs.insert("root".into(), root);
    client.logs.insert("child".into(), child);
    let value = serde_json::to_value(client.conversations()).unwrap();
    assert!(value
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "child" && c["internal"] == true));
}

#[test]
fn browser_announcement_replay_rejects_member_versions_after_publish() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let identity = NativeSigner::from_secret(&[71; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[72; 32]));
    let author = Author::device(&identity, host.0.clone());
    let member = Author::root(NativeSigner::from_secret(&[73; 32]));
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let mut log = SpaceLog::new("root");
    clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Community".into(),
            kind: SpaceKind::Community,
            privacy: Privacy::Closed,
        },
        1,
    );
    let content = |text: &str| {
        roda_types::ItemContent::Page(roda_types::PageContent {
            title: text.into(),
            path: String::new(),
            loro: STANDARD.encode(roda_docs::Page::from_markdown(text).unwrap().snapshot()),
            blob: None,
            manifest: None,
        })
    };
    clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: "page".into(),
            kind: roda_types::ItemKind::Page,
            content: content("Original"),
            origin: "test".into(),
        },
        2,
    );
    clear(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "publish".into(),
            text: "zoen-living-space-meta-v1:{\"action\":\"announce\",\"item_id\":\"page\"}".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
        3,
    );
    clear(
        &mut log,
        &author,
        EventBody::MemberAdded {
            identity: member.identity.clone(),
            role: Role::Member,
        },
        3,
    );
    clear(
        &mut log,
        &member,
        EventBody::ItemVersioned {
            item: "page".into(),
            content: content("Unauthorized"),
            note: String::new(),
        },
        4,
    );
    client.logs.insert("root".into(), log);
    assert_eq!(
        client.page("page").unwrap()["blocks"][0]["text"],
        "Original"
    );
    assert_eq!(
        client.living_space_home("root").unwrap()["page_details"][0]["title"],
        "Original"
    );
}

#[test]
fn browser_live_page_edits_survive_snapshot_and_merge_stale_editor() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde_json::{json, Value};
    let identity = NativeSigner::from_secret(&[71; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[72; 32]));
    let author = Author::device(&identity, host.0.clone());
    let account = account(&identity, &host.0);
    let mut client = Client::new(account.clone(), host.clone(), "").unwrap();
    let mut log = SpaceLog::new("sp_browser_page");
    clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Pages".into(),
            kind: SpaceKind::Community,
            privacy: Privacy::Closed,
        },
        1,
    );
    let page = roda_docs::Page::from_markdown("# Home\n\nFirst\n\nSecond\n").unwrap();
    clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: "page_test".into(),
            kind: roda_types::ItemKind::Page,
            content: roda_types::ItemContent::Page(roda_types::PageContent {
                title: "Home".into(),
                path: String::new(),
                loro: STANDARD.encode(page.snapshot()),
                blob: None,
                manifest: None,
            }),
            origin: "Test".into(),
        },
        2,
    );
    client.logs.insert(log.space().into(), log.clone());
    client.synced = true;
    let read = client.page("page_test").unwrap();
    let original = read["blocks"].as_array().unwrap().clone();
    let request = |id: &str, index: usize, text: &str| {
        let mut blocks = original.clone();
        blocks[index]["text"] = json!(text);
        json!({"item_id":"page_test","edit_id":id,"edit_context":read["edit_context"],"base_blocks":original,"blocks":blocks,"order":blocks.iter().map(|b|b["id"].clone()).collect::<Vec<_>>(),"changed":[blocks[index].clone()]}).to_string()
    };
    let first = request("browser-edit-one", 1, "First edited");
    let one = client.page_apply(&first, 3).unwrap();
    assert!(one
        .frames
        .iter()
        .any(|bytes| matches!(ClientFrame::decode(bytes), Ok(ClientFrame::Publish { .. }))));
    let two = client
        .page_apply(&request("browser-edit-two", 2, "Second edited"), 4)
        .unwrap();
    let merged = client.page("page_test").unwrap();
    assert_eq!(merged["blocks"][1]["text"], "First edited");
    assert_eq!(merged["blocks"][2]["text"], "Second edited");
    let mut restored = Client::new(account, host, &two.snapshot).unwrap();
    restored.synced = true;
    assert_eq!(
        restored.page("page_test").unwrap()["blocks"],
        merged["blocks"]
    );
    let pending_before = restored.saved.outbox.len();
    restored.page_apply(&first, 5).unwrap();
    assert_eq!(
        restored.saved.outbox.len(),
        pending_before,
        "same edit ID is idempotent"
    );
    let mut wrong: Value = serde_json::from_str(&first).unwrap();
    wrong["changed"][0]["text"] = json!("collision");
    assert!(restored.page_apply(&wrong.to_string(), 6).is_err());
    let failed = restored.page("page_test").unwrap();
    assert_eq!(failed["unsaved"], true);
    assert_eq!(failed["draft"]["changed"][0]["text"], "collision");
    assert!(failed["save_error"]
        .as_str()
        .unwrap()
        .contains("Edit ID reused"));
    let snapshot = restored.update(vec![]).unwrap().snapshot;
    let mut restored =
        Client::new(restored.account.clone(), restored.signer.clone(), &snapshot).unwrap();
    restored.synced = true;
    assert_eq!(
        restored.page("page_test").unwrap()["draft"],
        failed["draft"]
    );

    // A permanent relay rejection is a recoverable draft, never confirmed page content.
    for pending in restored.saved.outbox.values_mut() {
        pending.failure = Some("Relay rejected edit".into());
    }
    let rejected = restored.page("page_test").unwrap();
    assert_eq!(rejected["save_error"], "Relay rejected edit");
    assert_eq!(rejected["blocks"][1]["text"], "First");
    restored.page_apply(&first, 7).unwrap();
    assert_eq!(
        restored.page("page_test").unwrap()["blocks"][1]["text"],
        "First edited"
    );
    assert!(restored.page("page_test").unwrap()["save_error"].is_null());
}

#[test]
fn browser_live_page_two_devices_converge_from_signed_relay_frames() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde_json::json;
    let identity = NativeSigner::from_secret(&[74; 32]);
    let host_a = HostSigner(NativeSigner::from_secret(&[75; 32]));
    let host_b = HostSigner(NativeSigner::from_secret(&[76; 32]));
    let author = Author::device(&identity, host_a.0.clone());
    let mut a = Client::new(account(&identity, &host_a.0), host_a, "").unwrap();
    let mut b = Client::new(account(&identity, &host_b.0), host_b, "").unwrap();
    let mut relay = SpaceLog::new("sp_page_converge");
    clear(
        &mut relay,
        &author,
        EventBody::SpaceCreated {
            title: "Pages".into(),
            kind: SpaceKind::Community,
            privacy: Privacy::Closed,
        },
        1,
    );
    let page = roda_docs::Page::from_markdown("# Home\n\nFirst\n\nSecond\n").unwrap();
    clear(
        &mut relay,
        &author,
        EventBody::ItemCreated {
            item: "page_converge".into(),
            kind: roda_types::ItemKind::Page,
            content: roda_types::ItemContent::Page(roda_types::PageContent {
                title: "Home".into(),
                path: String::new(),
                loro: STANDARD.encode(page.snapshot()),
                blob: None,
                manifest: None,
            }),
            origin: "Test".into(),
        },
        2,
    );
    a.logs.insert(relay.space().into(), relay.clone());
    b.logs.insert(relay.space().into(), relay.clone());
    a.synced = true;
    b.synced = true;
    let read = a.page("page_converge").unwrap();
    let original = read["blocks"].as_array().unwrap().clone();
    let request = |id: &str, index: usize, text: &str| {
        let mut blocks = original.clone();
        blocks[index]["text"] = json!(text);
        json!({"item_id":"page_converge","edit_id":id,"edit_context":read["edit_context"],"base_blocks":original,"blocks":blocks,"order":blocks.iter().map(|b|b["id"].clone()).collect::<Vec<_>>(),"changed":[blocks[index].clone()]}).to_string()
    };
    let from_a = a.page_apply(&request("device-a", 1, "From A"), 3).unwrap();
    let from_b = b.page_apply(&request("device-b", 2, "From B"), 4).unwrap();
    for bytes in from_a.frames.into_iter().chain(from_b.frames) {
        if let Ok(ClientFrame::Publish { env }) = ClientFrame::decode(&bytes) {
            let ordered = sequence(&mut relay, env);
            a.receive(&ordered, 5).unwrap();
            b.receive(&ordered, 5).unwrap();
        }
    }
    let final_a = a.page("page_converge").unwrap();
    let final_b = b.page("page_converge").unwrap();
    assert_eq!(final_a["blocks"], final_b["blocks"]);
    assert_eq!(final_a["blocks"][1]["text"], "From A");
    assert_eq!(final_a["blocks"][2]["text"], "From B");
    assert_eq!(final_a["content_hash"], final_b["content_hash"]);
}

#[test]
fn browser_file_upload_plan_signs_blobs_and_commits_into_actual_mls_child() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde_json::json;
    let identity = NativeSigner::from_secret(&[91; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[92; 32]));
    let author = Author::device(&identity, host.0.clone());
    let mut client = Client::new(account(&identity, &host.0), host.clone(), "").unwrap();
    let mut root = SpaceLog::new("sp_browser_files");
    clear(
        &mut root,
        &author,
        EventBody::SpaceCreated {
            title: "Files".into(),
            kind: SpaceKind::Community,
            privacy: Privacy::Closed,
        },
        1,
    );
    client.logs.insert(root.space().into(), root.clone());
    client.synced = true;
    let mutation = client
        .living_space_mutate(
            root.space(),
            &json!({"action":"create_child","title":"Files"}).to_string(),
            2,
            "file-child",
        )
        .unwrap();
    let child = mutation.result["space"].as_str().unwrap().to_string();
    let plain = b"Shared encrypted file bytes";
    let plan = client
        .file_prepare(&child, "notes.txt", "text/plain", plain, 3)
        .unwrap();
    let mut packed = vec![];
    for upload in plan["uploads"].as_array().unwrap() {
        let blob = upload["blob"].as_str().unwrap();
        let headers = &upload["headers"];
        assert!(roda_log::verify_sig(
            &host.0.id(),
            &roda_proto::blob_put_message(blob, 3, &client.account.relay),
            headers["x-zoen-sig"].as_str().unwrap()
        ));
        packed.extend(
            STANDARD
                .decode(upload["body_base64"].as_str().unwrap())
                .unwrap(),
        );
    }
    let update = client
        .file_commit(&child, &plan.to_string(), 4, "file-commit")
        .unwrap();
    assert!(!update.frames.is_empty());
    let item = plan["item"].as_str().unwrap();
    assert_eq!(client.file_manifest(item).unwrap()["bytes"], plain.len());
    assert_eq!(client.file_open(item, &packed).unwrap(), plain);
    let mut corrupted = packed.clone();
    corrupted[24] ^= 1;
    assert!(client.file_open(item, &corrupted).is_err());
    let before = client.saved.outbox.len();
    client
        .file_commit(&child, &plan.to_string(), 5, "file-commit")
        .unwrap();
    assert_eq!(before, client.saved.outbox.len());
}

#[test]
fn browser_page_create_is_encrypted_idempotent_and_survives_snapshot_reload() {
    use serde_json::json;
    let identity = NativeSigner::from_secret(&[101; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[102; 32]));
    let author = Author::device(&identity, host.0.clone());
    let mut client = Client::new(account(&identity, &host.0), host.clone(), "").unwrap();
    let mut root = SpaceLog::new("sp_browser_page_creation");
    clear(
        &mut root,
        &author,
        EventBody::SpaceCreated {
            title: "Community".into(),
            kind: SpaceKind::Community,
            privacy: Privacy::Closed,
        },
        1,
    );
    client.logs.insert(root.space().into(), root.clone());
    client.synced = true;
    let child = client
        .living_space_mutate(
            root.space(),
            &json!({"action":"create_child","title":"Private work"}).to_string(),
            2,
            "page-child",
        )
        .unwrap()
        .result["space"]
        .as_str()
        .unwrap()
        .to_owned();
    let created = client
        .page_create(&child, "Private plan", 3, "new-page")
        .unwrap();
    let item = created.result["item_id"].as_str().unwrap();
    assert_eq!(created.result["title"], "Private plan");
    assert_eq!(created.result["can_edit"], true);
    assert_eq!(created.result["pending_sync"], true);
    assert!(!created.update.frames.is_empty());
    let count = client.saved.outbox.len();
    let retry = client
        .page_create(&child, "Private plan", 4, "new-page")
        .unwrap();
    assert_eq!(retry.result["item_id"], item);
    assert_eq!(client.saved.outbox.len(), count);
    assert!(client
        .page_create(&child, "Different content", 4, "new-page")
        .is_err());
    assert!(client.page_create(&child, "Private plan", 4, "").is_err());
    let restored =
        Client::new(account(&identity, &host.0), host, &created.update.snapshot).unwrap();
    assert_eq!(restored.page(item).unwrap()["title"], "Private plan");
    assert_eq!(
        restored.page(item).unwrap()["content_hash"],
        created.result["content_hash"]
    );
}

#[test]
fn browser_live_page_downloads_authenticated_blob_before_editing() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305, XNonce,
    };
    use sha2::{Digest, Sha256};
    let identity = NativeSigner::from_secret(&[93; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[94; 32]));
    let author = Author::device(&identity, host.0.clone());
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let mut root = SpaceLog::new("sp_blob_page");
    clear(
        &mut root,
        &author,
        EventBody::SpaceCreated {
            title: "Pages".into(),
            kind: SpaceKind::Community,
            privacy: Privacy::Closed,
        },
        1,
    );
    let page = roda_docs::Page::from_markdown("# Blob-backed home\n\nActual shared Loro blocks\n")
        .unwrap();
    let plain = page.snapshot();
    let key = [7u8; 32];
    let nonce = [8u8; 24];
    let cipher = XChaCha20Poly1305::new_from_slice(&key).unwrap();
    let mut sealed = nonce.to_vec();
    sealed.extend(
        cipher
            .encrypt(&XNonce::from(nonce), plain.as_slice())
            .unwrap(),
    );
    let blob = hex::encode(Sha256::digest(&sealed));
    let reference = roda_types::ChunkRef {
        sha256: hex::encode(Sha256::digest(&plain)),
        bytes: plain.len() as u64,
        blob: blob.clone(),
        key: hex::encode(key),
    };
    clear(
        &mut root,
        &author,
        EventBody::ItemCreated {
            item: "page_blob_test".into(),
            kind: roda_types::ItemKind::Page,
            content: roda_types::ItemContent::Page(roda_types::PageContent {
                title: "Blob-backed home".into(),
                path: String::new(),
                loro: String::new(),
                blob: Some(reference),
                manifest: None,
            }),
            origin: "Test".into(),
        },
        2,
    );
    client.logs.insert(root.space().into(), root);
    client.synced = true;
    assert!(client.page("page_blob_test").is_err());
    assert_eq!(
        client.page_download_manifest("page_blob_test").unwrap()["ready"],
        false
    );
    let mut corrupted = sealed.clone();
    corrupted[25] ^= 1;
    assert!(client
        .page_blob_supply("page_blob_test", &blob, &corrupted)
        .is_err());
    client
        .page_blob_supply("page_blob_test", &blob, &sealed)
        .unwrap();
    assert_eq!(
        client.page_download_manifest("page_blob_test").unwrap()["ready"],
        true
    );
    assert_eq!(
        client.page("page_blob_test").unwrap()["blocks"][1]["text"],
        "Actual shared Loro blocks"
    );
    assert!(STANDARD.encode(&sealed).len() > plain.len());
}

fn web_fixture() -> (Client<HostSigner>, Author, Author, SpaceLog, String) {
    let identity = NativeSigner::from_secret(&[41; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[42; 32]));
    let author = Author::device(&identity, NativeSigner::from_secret(&[43; 32]));
    let other = Author::device(
        &NativeSigner::from_secret(&[44; 32]),
        NativeSigner::from_secret(&[45; 32]),
    );
    let client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let mut log = SpaceLog::new("sp_web_receipts");
    clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
        1,
    );
    clear(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_fixture".into(),
            text: "Oi".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
        2,
    );
    let target = log.events().last().unwrap().hash.clone();
    (client, author, other, log, target)
}

fn annotate(log: &mut SpaceLog, author: &Author, target: &str, note: roda_types::Annotation) {
    let now = log.next_seq() as i64 + 10;
    clear(
        log,
        author,
        EventBody::MessageAnnotated {
            target: target.into(),
            note,
        },
        now,
    );
}

#[test]
fn web_reaction_participants_replace_remove_and_ignore_invalid_reactions() {
    let (mut client, author, other, mut log, target) = web_fixture();
    let react = |emoji: &str| roda_types::Annotation::React {
        emoji: emoji.into(),
    };
    annotate(&mut log, &author, &target, react("👍"));
    annotate(&mut log, &other, &target, react("👍"));
    client.logs.insert(log.space().into(), log.clone());
    let view = client.conversations();
    let reaction = &view[0].messages[0].reactions[0];
    assert_eq!(reaction.count, 2);
    assert_eq!(reaction.participants.len(), 2);
    assert!(reaction.mine);
    assert!(reaction
        .participants
        .iter()
        .any(|p| p.identity == client.account.identity && p.name == "Fixture"));
    annotate(&mut log, &author, &target, react("❤️"));
    annotate(&mut log, &other, &target, react(" "));
    annotate(&mut log, &other, &target, react(&"😀".repeat(9)));
    client.logs.insert(log.space().into(), log.clone());
    let view = client.conversations();
    assert_eq!(view[0].messages[0].reactions.len(), 2);
    annotate(&mut log, &author, &target, roda_types::Annotation::Unreact);
    client.logs.insert(log.space().into(), log.clone());
    let view = client.conversations();
    let reactions = &view[0].messages[0].reactions;
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].emoji, "👍");
    assert_eq!(reactions[0].count, 1);
    assert!(!reactions[0].mine);
    annotate(&mut log, &author, &target, roda_types::Annotation::Delete);
    annotate(&mut log, &other, &target, react("❤️"));
    client.logs.insert(log.space().into(), log);
    assert!(client.conversations()[0].messages[0].reactions.is_empty());
    assert!(client
        .queue_annotation("sp_web_receipts", &target, react("👍"), 90)
        .err()
        .unwrap()
        .contains("no longer available"));
}

#[test]
fn web_reaction_submission_rejects_blank_and_oversized_utf8_before_encryption() {
    let (mut client, _, _, log, target) = web_fixture();
    client.logs.insert(log.space().into(), log);
    for emoji in [" ".into(), "😀".repeat(9)] {
        assert!(client
            .queue_annotation(
                "sp_web_receipts",
                &target,
                roda_types::Annotation::React { emoji },
                90
            )
            .err()
            .unwrap()
            .contains("32 bytes"));
    }
}

fn receipt_marks(frames: &[Vec<u8>]) -> Vec<(&'static str, u64)> {
    frames
        .iter()
        .filter_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Ephemeral {
                kind: roda_proto::EphemeralKind::Delivered { seq },
                ..
            } => Some(("delivered", seq)),
            ClientFrame::Ephemeral {
                kind: roda_proto::EphemeralKind::Read { seq },
                ..
            } => Some(("read", seq)),
            _ => None,
        })
        .collect()
}

#[test]
fn web_receipts_replay_after_saved_unsent_frames_and_each_reconnect() {
    let (mut client, _, _, log, _) = web_fixture();
    let space = log.space().to_owned();
    let last = log.next_seq() - 1;
    client.logs.insert(space.clone(), log);
    let first = client.mark_read(&space).unwrap(); // Simulate a durable save, then losing the outgoing frame.
    assert_eq!(receipt_marks(&first.frames), vec![("read", last)]);
    assert_eq!(
        receipt_marks(&client.mark_read(&space).unwrap().frames),
        vec![("read", last)]
    );
    let mut restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &first.snapshot,
    )
    .unwrap();
    for _ in 0..2 {
        restored.bootstrap().unwrap();
        let update = restored
            .receive(&ServerFrame::SyncDone.encode(), 100)
            .unwrap();
        assert_eq!(
            receipt_marks(&update.frames),
            vec![("delivered", last), ("read", last)]
        );
        assert!(restored.delivered().is_empty());
    }
}
