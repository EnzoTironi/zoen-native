use super::*;
use types::{Annotation, ReplyRef};

const SPACE: &str = "sp_mutation_fixture";

// Reuse the existing fixture's real SQLite MLS device, signing and relay sequencing.
fn fixture(run: impl FnOnce(&mut Client<HostSigner>, &Device<'_>, &Author, &mut SpaceLog)) {
    let identity = NativeSigner::from_secret(&[41; 32]);
    let peer_identity = NativeSigner::from_secret(&[42; 32]);
    let native_key = NativeSigner::from_secret(&[43; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[44; 32]));
    let author = Author::device(&peer_identity, native_key.clone());
    let mut sqlite = rusqlite::Connection::open_in_memory().unwrap();
    mls::migrate(&mut sqlite).unwrap();
    let native = Device::new(
        &sqlite,
        [45; 32],
        &peer_identity.id(),
        native_key.secret(),
        author.cert.as_deref().unwrap(),
    )
    .unwrap();
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
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
    let mut log = SpaceLog::new(SPACE);
    let create = clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Mutation fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
        1,
    );
    client.receive(&create, 2).unwrap();
    let member = clear(
        &mut log,
        &author,
        EventBody::MemberAdded {
            identity: identity.id(),
            role: Role::Member,
        },
        3,
    );
    client.receive(&member, 4).unwrap();
    native.create_group(SPACE).unwrap();
    let roster = BTreeSet::from([identity.id(), peer_identity.id()]);
    let commit = native.commit(SPACE, &[package], &BTreeSet::new()).unwrap();
    native
        .open(SPACE, &commit.commit, &roster, native.leaf())
        .unwrap();
    client
        .receive(
            &sealed(&mut log, &author, SealedKind::Commit, commit.commit, 5),
            6,
        )
        .unwrap();
    client
        .receive(
            &sealed(
                &mut log,
                &author,
                SealedKind::Welcome,
                commit.welcome.unwrap(),
                7,
            ),
            8,
        )
        .unwrap();
    client.receive(&ServerFrame::SyncDone.encode(), 9).unwrap();
    assert!(client.conversations()[0].can_send);
    run(&mut client, &native, &author, &mut log);
}

fn accept(client: &mut Client<HostSigner>, log: &mut SpaceLog, update: Update, now: i64) -> String {
    let env = update
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
        .expect("encrypted application queued");
    env.verify().unwrap();
    let frame = sequence(log, env);
    let hash = match ServerFrame::decode(&frame).unwrap() {
        ServerFrame::Event { ev } => ev.hash,
        _ => unreachable!(),
    };
    client.receive(&frame, now).unwrap();
    hash
}

fn peer_event(
    client: &mut Client<HostSigner>,
    native: &Device<'_>,
    author: &Author,
    log: &mut SpaceLog,
    body: EventBody,
    now: i64,
) -> String {
    let inner = author.sign_event(SPACE, &types::new_ulid(now), now, log.head(), body);
    let plaintext = InnerEvent {
        content: inner.content,
        sig: inner.sig,
    }
    .encode();
    let ciphertext = native.seal(SPACE, &plaintext).unwrap();
    let (content, sig) = author.sign_sealed(
        SPACE,
        &inner.client_id,
        now,
        inner.seen.as_ref(),
        Sealed::new(SealedKind::Application, SUITE_ID, ciphertext),
    );
    let frame = sequence(
        log,
        Envelope::new(content, sig, author.cert.clone(), None).unwrap(),
    );
    let hash = match ServerFrame::decode(&frame).unwrap() {
        ServerFrame::Event { ev } => ev.hash,
        _ => unreachable!(),
    };
    client.receive(&frame, now + 1).unwrap();
    hash
}

fn reload(client: &mut Client<HostSigner>) {
    let snapshot = client.update(Vec::new()).unwrap().snapshot;
    let mut restored =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    restored.bootstrap().unwrap();
    restored
        .receive(&ServerFrame::SyncDone.encode(), 1000)
        .unwrap();
    *client = restored;
}

fn assert_folded(client: &Client<HostSigner>, target: &str, text: &str, deleted: bool) {
    let conversations = client.conversations();
    let messages = &conversations[0].messages;
    let root = messages
        .iter()
        .find(|m| m.entry.as_deref() == Some(target))
        .unwrap();
    assert_eq!(root.text, text);
    assert_eq!(root.deleted, deleted);
    assert_eq!(root.replies, 1);
    let quote = messages.iter().find_map(|m| m.quote.as_ref()).unwrap();
    assert_eq!(quote.id, target);
    assert_eq!(quote.text, text);
    assert!(messages.iter().any(|m| m.thread.as_deref() == Some(target)));
    assert!(!messages.iter().any(|m| m.text.contains("oldterm")));
    assert!(!messages
        .iter()
        .any(|m| m.quote.as_ref().is_some_and(|q| q.text.contains("oldterm"))));
    if deleted {
        assert!(!messages.iter().any(|m| m.text.contains("newterm")));
        assert!(!messages
            .iter()
            .any(|m| m.quote.as_ref().is_some_and(|q| q.text.contains("newterm"))));
    }
}

#[test]
fn edit_delete_fold_and_reload_update_text_quotes_and_thread_references() {
    fixture(|client, _, _, log| {
        let update = client.queue_message(SPACE, "oldterm", 10).unwrap();
        let target = accept(client, log, update, 11);
        for (text, thread, now) in [("quote", false, 12), ("thread", true, 14)] {
            let update = client
                .queue_post(SPACE, text, Some((target.clone(), thread)), false, now)
                .unwrap();
            accept(client, log, update, now + 1);
        }
        let update = client
            .queue_annotation(
                SPACE,
                &target,
                Annotation::Edit {
                    text: "newterm".into(),
                },
                16,
            )
            .unwrap();
        accept(client, log, update, 17);
        assert_folded(client, &target, "newterm", false);
        reload(client);
        assert_folded(client, &target, "newterm", false);
        let update = client
            .queue_annotation(SPACE, &target, Annotation::Delete, 18)
            .unwrap();
        accept(client, log, update, 19);
        assert_folded(client, &target, "", true);
        reload(client);
        assert_folded(client, &target, "", true);
    });
}

#[test]
fn rejects_late_edits_after_confirmed_deletion_and_reload_without_queuing() {
    fixture(|client, _, _, log| {
        let update = client.queue_message(SPACE, "oldterm", 10).unwrap();
        let target = accept(client, log, update, 11);
        let update = client
            .queue_annotation(SPACE, &target, Annotation::Delete, 12)
            .unwrap();
        accept(client, log, update, 13);
        for restored in [false, true] {
            if restored {
                reload(client);
            }
            let before = client.saved.outbox.len();
            let result = client.queue_annotation(
                SPACE,
                &target,
                Annotation::Edit {
                    text: "late newterm".into(),
                },
                14,
            );
            assert!(
                result.is_err(),
                "a deleted target must not report a successful edit"
            );
            assert_eq!(client.saved.outbox.len(), before);
            let conversations = client.conversations();
            assert!(conversations[0].messages[0].deleted);
            assert!(conversations[0].messages[0].text.is_empty());
        }
    });
}

#[test]
fn pending_delete_blocks_later_author_mutations_including_after_reload() {
    fixture(|client, _, _, log| {
        let update = client.queue_message(SPACE, "original", 10).unwrap();
        let target = accept(client, log, update, 11);
        client
            .queue_annotation(SPACE, &target, Annotation::Delete, 12)
            .unwrap();
        for restored in [false, true] {
            if restored {
                reload(client);
            }
            for note in [
                Annotation::Edit {
                    text: "late".into(),
                },
                Annotation::Delete,
            ] {
                let before = client.saved.outbox.len();
                assert!(client.queue_annotation(SPACE, &target, note, 14).is_err());
                assert_eq!(client.saved.outbox.len(), before);
            }
        }
    });
}

#[test]
fn permanently_rejected_delete_allows_edit_before_and_after_reload() {
    fixture(|client, _, _, log| {
        let update = client.queue_message(SPACE, "original", 10).unwrap();
        let target = accept(client, log, update, 11);
        client
            .queue_annotation(SPACE, &target, Annotation::Delete, 12)
            .unwrap();
        let client_id = client
            .saved
            .outbox
            .iter()
            .find_map(|(id, pending)| {
                pending.inner.as_ref().filter(|event| matches!(
                &event.body,
                EventBody::MessageAnnotated { target: deleted, note: Annotation::Delete }
                    if deleted == &target
            )).map(|_| id.clone())
            })
            .expect("queued deletion");
        client
            .receive(
                &ServerFrame::Rejected {
                    space: SPACE.into(),
                    client_id: client_id.clone(),
                    reason: "delete rejected".into(),
                    permanent: true,
                }
                .encode(),
                13,
            )
            .unwrap();
        for restored in [false, true] {
            if restored {
                reload(client);
            }
            assert!(client.saved.outbox[&client_id].failure.is_some());
            let update = client
                .queue_annotation(
                    SPACE,
                    &target,
                    Annotation::Edit {
                        text: "replacement".into(),
                    },
                    14,
                )
                .expect("a permanently rejected delete must not block a valid edit");
            accept(client, log, update, 15);
            let conversations = client.conversations();
            assert_eq!(conversations[0].messages[0].text, "replacement");
            assert!(!conversations[0].messages[0].deleted);
        }
    });
}

#[test]
fn unauthorized_mutations_are_rejected_and_signed_peer_mutations_do_not_fold() {
    fixture(|client, native, author, log| {
        let update = client.queue_message(SPACE, "mine", 10).unwrap();
        let mine = accept(client, log, update, 11);
        for (note, now) in [
            (
                Annotation::Edit {
                    text: "forged".into(),
                },
                12,
            ),
            (Annotation::Delete, 14),
        ] {
            peer_event(
                client,
                native,
                author,
                log,
                EventBody::MessageAnnotated {
                    target: mine.clone(),
                    note,
                },
                now,
            );
        }
        let peer = peer_event(
            client,
            native,
            author,
            log,
            EventBody::MessagePosted {
                message: "msg_peer".into(),
                text: "theirs".into(),
                attaches: None,
                reply: Some(ReplyRef {
                    to: mine.clone(),
                    thread: false,
                }),
                forwarded: false,
            },
            16,
        );
        for note in [
            Annotation::Edit {
                text: "not mine".into(),
            },
            Annotation::Delete,
        ] {
            let before = client.saved.outbox.len();
            assert!(client
                .queue_annotation(SPACE, &peer, note, 18)
                .err()
                .unwrap()
                .contains("Only the person"));
            assert_eq!(client.saved.outbox.len(), before);
        }
        for restored in [false, true] {
            if restored {
                reload(client);
            }
            let conversations = client.conversations();
            let own = conversations[0]
                .messages
                .iter()
                .find(|m| m.entry.as_deref() == Some(&mine))
                .unwrap();
            assert_eq!(own.text, "mine");
            assert!(!own.deleted);
            assert_eq!(own.edited, restored);
            // An unauthorized deletion must not block the author's own valid edit.
            let update = client
                .queue_annotation(
                    SPACE,
                    &mine,
                    Annotation::Edit {
                        text: "mine".into(),
                    },
                    20,
                )
                .unwrap();
            accept(client, log, update, 21);
        }
    });
}

#[test]
fn signed_late_author_edit_received_after_deletion_cannot_resurrect_message() {
    fixture(|client, native, author, log| {
        let target = peer_event(
            client,
            native,
            author,
            log,
            EventBody::MessagePosted {
                message: "msg_peer".into(),
                text: "oldterm".into(),
                attaches: None,
                reply: None,
                forwarded: false,
            },
            10,
        );
        for (note, now) in [
            (Annotation::Delete, 12),
            (
                Annotation::Edit {
                    text: "newterm".into(),
                },
                14,
            ),
        ] {
            peer_event(
                client,
                native,
                author,
                log,
                EventBody::MessageAnnotated {
                    target: target.clone(),
                    note,
                },
                now,
            );
        }
        for restored in [false, true] {
            if restored {
                reload(client);
            }
            let conversations = client.conversations();
            let message = conversations[0]
                .messages
                .iter()
                .find(|m| m.entry.as_deref() == Some(&target))
                .unwrap();
            assert!(message.deleted);
            assert!(message.text.is_empty());
            assert!(!message.edited);
        }
    });
}

#[test]
fn invalid_edits_and_missing_or_non_message_targets_do_not_queue() {
    fixture(|client, _, _, log| {
        let update = client.queue_message(SPACE, "original", 10).unwrap();
        let target = accept(client, log, update, 11);
        for text in [
            "".to_string(),
            " \n\t".to_string(),
            "a".repeat(32_769),
            "é".repeat(16_385),
        ] {
            let before = client.saved.outbox.len();
            assert!(client
                .queue_annotation(SPACE, &target, Annotation::Edit { text }, 12)
                .is_err());
            assert_eq!(client.saved.outbox.len(), before);
        }
        let not_message = log.events()[0].hash.clone();
        for target in ["missing", not_message.as_str()] {
            for note in [
                Annotation::Edit {
                    text: "valid".into(),
                },
                Annotation::Delete,
            ] {
                assert!(client.queue_annotation(SPACE, target, note, 13).is_err());
            }
        }
        let update = client
            .queue_annotation(
                SPACE,
                &target,
                Annotation::Edit {
                    text: "é".repeat(16_384),
                },
                14,
            )
            .unwrap();
        accept(client, log, update, 15);
        assert_eq!(client.conversations()[0].messages[0].text.len(), 32_768);
    });
}
