//! Synthetic native MLS tail retained without a subsequent live wake-up.
use super::*;

const SPACE: &str = "sp_browser_final_lost";
const TEXT: &str = "O último evento retido também precisa chegar. 🪴";

struct Fixture {
    sqlite: rusqlite::Connection,
    native_key: NativeSigner,
    author: Author,
    client: Client<HostSigner>,
    log: SpaceLog,
}
impl Fixture {
    fn new() -> Self {
        let identity = NativeSigner::from_secret(&[111; 32]);
        let native_key = NativeSigner::from_secret(&[112; 32]);
        let host = HostSigner(NativeSigner::from_secret(&[113; 32]));
        let author = Author::device(&identity, native_key.clone());
        let mut sqlite = rusqlite::Connection::open_in_memory().unwrap();
        mls::migrate(&mut sqlite).unwrap();
        let native = Device::new(
            &sqlite,
            [114; 32],
            &identity.id(),
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
        let created = clear(
            &mut log,
            &author,
            EventBody::SpaceCreated {
                title: "Lost final event".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
            1,
        );
        client.receive(&created, 2).unwrap();
        native.create_group(SPACE).unwrap();
        let roster = BTreeSet::from([identity.id()]);
        let joined = native.commit(SPACE, &[package], &BTreeSet::new()).unwrap();
        native
            .open(SPACE, &joined.commit, &roster, native.leaf())
            .unwrap();
        client
            .receive(
                &sealed(&mut log, &author, SealedKind::Commit, joined.commit, 4),
                5,
            )
            .unwrap();
        client
            .receive(
                &sealed(
                    &mut log,
                    &author,
                    SealedKind::Welcome,
                    joined.welcome.unwrap(),
                    6,
                ),
                7,
            )
            .unwrap();
        client.receive(&ServerFrame::SyncDone.encode(), 8).unwrap();
        assert!(client.conversations()[0].can_send);
        assert!(client.conversations()[0].messages.is_empty());
        drop(native);
        Self {
            sqlite,
            native_key,
            author,
            client,
            log,
        }
    }

    fn retained_tail(&mut self) -> Vec<u8> {
        let inner = self.author.sign_event(
            SPACE,
            "final-retained-message",
            9,
            self.log.head(),
            EventBody::MessagePosted {
                message: "msg_final_retained".into(),
                text: TEXT.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        );
        let plaintext = InnerEvent {
            content: inner.content,
            sig: inner.sig,
        }
        .encode();
        let native = Device::new(
            &self.sqlite,
            [114; 32],
            &self.author.identity,
            self.native_key.secret(),
            self.author.cert.as_deref().unwrap(),
        )
        .unwrap();
        let ciphertext = native.seal(SPACE, &plaintext).unwrap();
        let (content, sig) = self.author.sign_sealed(
            SPACE,
            &inner.client_id,
            9,
            inner.seen.as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, ciphertext),
        );
        sequence(
            &mut self.log,
            Envelope::new(content, sig, self.author.cert.clone(), None).unwrap(),
        )
    }

    fn assert_tail(&self) {
        assert_eq!(self.client.logs[SPACE].next_seq(), self.log.next_seq());
        let views = self.client.conversations();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].messages.len(), 1);
        assert_eq!(views[0].messages[0].id, "final-retained-message");
        assert_eq!(views[0].messages[0].text, TEXT);
        assert_eq!(views[0].messages[0].seq, Some(3));
        assert_eq!(views[0].messages[0].delivery, "confirmed");
        SpaceLog::from_events(SPACE, self.client.logs[SPACE].events().to_vec()).unwrap();
    }
}

#[test]
fn retained_native_mls_tail_fixture_is_verified_exactly_once_and_survives_cold_restore() {
    let mut f = Fixture::new();
    let tail = f.retained_tail();
    f.client.receive(&tail, 10).unwrap();
    f.assert_tail();
    let update = f.client.receive(&tail, 11).unwrap();
    f.assert_tail();
    let account = f.client.account.clone();
    let signer = f.client.signer.clone();
    f.client = Client::new(account, signer, &update.snapshot).unwrap();
    f.assert_tail();
}

#[test]
fn forged_retained_tail_never_advances_the_browser_cursor() {
    let mut f = Fixture::new();
    let tail = f.retained_tail();
    let ServerFrame::Event { mut ev } = ServerFrame::decode(&tail).unwrap() else {
        panic!("fixture must retain an actual event")
    };
    let next = f.client.logs[SPACE].next_seq();
    ev.env.sig.replace_range(0..2, "00");
    assert!(f
        .client
        .receive(&ServerFrame::Event { ev }.encode(), 10)
        .is_err());
    assert_eq!(f.client.logs[SPACE].next_seq(), next);
    assert!(f.client.conversations()[0].messages.is_empty());
}

#[test]
fn healthy_browser_requests_the_unannounced_retained_final_event() {
    let mut f = Fixture::new();
    let tail = f.retained_tail();
    let next = f.client.logs[SPACE].next_seq();
    assert_eq!(next, 3);
    // No new Event, Joined, control notice, Pong or reconnect reaches this client.
    // A separate monotonic tick admits the repair, without UI/crypto maintenance.
    let crate::CursorRepairTick::Update { update } =
        f.client.reconcile(Duration::from_millis(60_008)).unwrap()
    else {
        panic!("the scheduled repair must fit the original deadline")
    };
    let cursors = update
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Sync { cursors, all: true } => Some(cursors),
            _ => None,
        })
        .expect("a healthy browser must reconcile without a later live wake-up");
    assert_eq!(
        cursors,
        vec![Cursor {
            space: SPACE.into(),
            next_seq: next
        }]
    );
    assert_eq!(
        update.cursor_repair.unwrap().abort_deadline_ms,
        Some(90_000)
    );
    f.client
        .receive_at(&tail, 10, Duration::from_millis(60_009))
        .unwrap();
    let saved = f
        .client
        .receive_at(
            &ServerFrame::SyncDone.encode(),
            11,
            Duration::from_millis(60_010),
        )
        .unwrap();
    f.assert_tail();
    f.client = Client::new(
        f.client.account.clone(),
        f.client.signer.clone(),
        &saved.snapshot,
    )
    .unwrap();
    f.assert_tail();
    let cold = f.client.bootstrap_at(Duration::from_millis(100)).unwrap();
    let sync = cold
        .frames
        .iter()
        .find_map(|b| match ClientFrame::decode(b).unwrap() {
            ClientFrame::Sync { cursors, all: true } => Some(cursors),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        sync,
        vec![Cursor {
            space: SPACE.into(),
            next_seq: 4
        }]
    );
    f.client
        .receive_at(&tail, 12, Duration::from_millis(101))
        .unwrap();
    f.client
        .receive_at(
            &ServerFrame::SyncDone.encode(),
            13,
            Duration::from_millis(102),
        )
        .unwrap();
    f.assert_tail();
}

#[test]
fn ordered_browser_partials_and_pong_cannot_finish_or_postpone_a_full_repair() {
    let mut f = Fixture::new();
    let clock = |ms| Duration::from_millis(ms);
    f.client
        .receive_at(
            &ServerFrame::Joined {
                space: SPACE.into(),
            }
            .encode(),
            9,
            clock(10),
        )
        .unwrap();
    let crate::CursorRepairTick::Update { update } = f.client.reconcile(clock(60_008)).unwrap()
    else {
        panic!("repair must be admitted")
    };
    assert_eq!(
        update.cursor_repair.unwrap().abort_deadline_ms,
        Some(90_000)
    );
    f.client
        .receive_at(
            &ServerFrame::Joined {
                space: SPACE.into(),
            }
            .encode(),
            10,
            clock(60_009),
        )
        .unwrap();
    let pong = f
        .client
        .receive_at(
            &ServerFrame::Pong {
                server_time_ms: None,
            }
            .encode(),
            11,
            clock(60_010),
        )
        .unwrap();
    assert_eq!(pong.cursor_repair.unwrap().abort_deadline_ms, Some(90_000));
    let partial = f
        .client
        .receive_at(&ServerFrame::SyncDone.encode(), 12, clock(60_011))
        .unwrap();
    assert_eq!(
        partial.cursor_repair.unwrap().abort_deadline_ms,
        Some(90_000)
    );
    let full = f
        .client
        .receive_at(&ServerFrame::SyncDone.encode(), 13, clock(60_012))
        .unwrap();
    assert_eq!(full.cursor_repair.unwrap().admission_ms, Some(120_012));
    let later = f
        .client
        .receive_at(&ServerFrame::SyncDone.encode(), 14, clock(65_000))
        .unwrap();
    assert_eq!(later.cursor_repair.unwrap().admission_ms, Some(120_012));
    let unsolicited = f
        .client
        .receive_at(&ServerFrame::SyncDone.encode(), 15, clock(70_000))
        .unwrap();
    assert_eq!(
        unsolicited.cursor_repair.unwrap().admission_ms,
        Some(120_012)
    );
}

#[test]
fn initial_verified_cursor_progress_renews_only_before_expiry_and_pong_cannot_do_so() {
    let mut f = Fixture::new();
    f.client.bootstrap_at(Duration::ZERO).unwrap();
    let tail = f.retained_tail();
    let progressed = f
        .client
        .receive_at(&tail, 10, Duration::from_millis(59_000))
        .unwrap();
    assert_eq!(
        progressed.cursor_repair.unwrap().abort_deadline_ms,
        Some(119_000)
    );
    let duplicate = f
        .client
        .receive_at(&tail, 11, Duration::from_millis(118_000))
        .unwrap();
    assert_eq!(
        duplicate.cursor_repair.unwrap().abort_deadline_ms,
        Some(119_000)
    );
    let pong = f
        .client
        .receive_at(
            &ServerFrame::Pong {
                server_time_ms: None,
            }
            .encode(),
            12,
            Duration::from_millis(118_999),
        )
        .unwrap();
    assert_eq!(pong.cursor_repair.unwrap().abort_deadline_ms, Some(119_000));
    assert!(matches!(
        f.client.reconcile(Duration::from_millis(119_000)).unwrap(),
        crate::CursorRepairTick::Expired
    ));
    assert!(matches!(
        f.client.reconcile(Duration::from_millis(119_001)).unwrap(),
        crate::CursorRepairTick::Expired
    ));
    f.assert_tail();
}

#[test]
fn elapsed_or_backwards_clock_cannot_emit_late_sync_or_revive_a_prior_socket() {
    let mut f = Fixture::new();
    assert!(matches!(
        f.client.reconcile(Duration::from_millis(90_000)).unwrap(),
        crate::CursorRepairTick::Expired
    ));
    assert!(f.client.reconcile(Duration::from_millis(89_999)).is_err());
    let snapshot = f.client.update(Vec::new()).unwrap().snapshot;
    f.client = Client::new(f.client.account.clone(), f.client.signer.clone(), &snapshot).unwrap();
    assert!(
        f.client.reconcile(Duration::ZERO).is_err(),
        "cold state has no socket owner"
    );
    let fresh = f.client.bootstrap_at(Duration::from_millis(7)).unwrap();
    assert_eq!(fresh.cursor_repair.unwrap().abort_deadline_ms, Some(60_007));
    let sync = fresh
        .frames
        .iter()
        .find_map(|b| match ClientFrame::decode(b).unwrap() {
            ClientFrame::Sync { cursors, all: true } => Some(cursors),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        sync,
        vec![Cursor {
            space: SPACE.into(),
            next_seq: 3
        }]
    );
}

#[test]
fn invalid_browser_clock_cannot_reset_the_existing_repair_owner_or_snapshot() {
    let mut f = Fixture::new();
    let before = f.client.update(Vec::new()).unwrap();
    assert!(f.client.bootstrap_at(Duration::MAX).is_err());
    assert!(f.client.bootstrap_at(Duration::from_nanos(1)).is_err());
    assert!(f.client.reconcile(Duration::MAX).is_err());
    let after = f.client.update(Vec::new()).unwrap();
    assert_eq!(before.snapshot, after.snapshot);
    assert_eq!(before.cursor_repair, after.cursor_repair);
    let config = f.client.cursor_repair_config();
    assert_eq!(
        (
            config.interval_ms,
            config.timeout_ms,
            config.initial_inactivity_ms
        ),
        (60_000, 30_000, 60_000)
    );
}

/// Export only this independently keyed, synthetic SQLite/MLS fixture for the
/// opt-in headless browser contract. The output is never a user/account vault.
#[test]
#[ignore = "requires an explicitly chosen new synthetic fixture output file"]
fn export_synthetic_cursor_fixture_for_real_browser_worker() {
    use std::io::Write;
    let path = std::env::var_os("ZOEN_BROWSER_CURSOR_FIXTURE_OUTPUT")
        .expect("an owned synthetic output path must be explicit");
    let mut f = Fixture::new();
    let snapshot = f.client.update(Vec::new()).unwrap().snapshot;
    let tail = f.retained_tail();
    let nonce = "synthetic-browser-cursor-challenge";
    let relay = "synthetic-browser-cursor-relay";
    let body = serde_json::json!({
        "format": "zoen-synthetic-browser-cursor-fixture-v1",
        "account": f.client.account,
        "snapshot": snapshot,
        "tail": tail,
        "space": SPACE,
        "expected_text": TEXT,
        "auth_message": protocol::auth_message(nonce, relay),
        "challenge": ServerFrame::Challenge { nonce: nonce.into(), relay: relay.into(),
            protocol: 4, capabilities: Vec::new(), server_time_ms: None }.encode(),
        "ready": ServerFrame::Ready { identity: f.author.identity.clone(), registered: true }.encode(),
        "sync_done": ServerFrame::SyncDone.encode(),
        "pong": ServerFrame::Pong { server_time_ms: None }.encode(),
    });
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).unwrap();
    file.write_all(&serde_json::to_vec(&body).unwrap()).unwrap();
    file.sync_all().unwrap();
}
