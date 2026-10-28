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
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let identity = NativeSigner::from_secret(&[111; 32]);
        let native_key = NativeSigner::from_secret(&[112; 32]);
        let host = HostSigner(NativeSigner::from_secret(&[113; 32]));
        let author = Author::device(&identity, native_key.clone());
        let mut sqlite = rusqlite::Connection::open_in_memory()?;
        mls::migrate(&mut sqlite)?;
        let native = Device::new(
            &sqlite,
            [114; 32],
            &identity.id(),
            native_key.secret(),
            author
                .cert
                .as_deref()
                .ok_or("missing authenticated test as_deref value")?,
        )?;
        let mut client = Client::new(account(&identity, &host.0), host, "")?;
        let start = client.bootstrap()?;
        let package = start
            .frames
            .iter()
            .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .find_map(|frame| match frame {
                ClientFrame::Req {
                    op: Op::PublishKeyPackages { packages, .. },
                    ..
                } => packages.into_iter().next(),
                _ => None,
            })
            .ok_or("missing authenticated test find_map value")?;
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
        )?;
        client.receive(&created, 2)?;
        native.create_group(SPACE)?;
        let roster = BTreeSet::from([identity.id()]);
        let joined = native.commit(SPACE, &[package], &BTreeSet::new())?;
        native.open(SPACE, &joined.commit, &roster, native.leaf())?;
        client.receive(
            &sealed(&mut log, &author, SealedKind::Commit, joined.commit, 4)?,
            5,
        )?;
        client.receive(
            &sealed(
                &mut log,
                &author,
                SealedKind::Welcome,
                joined.welcome.ok_or("missing authenticated test welcome")?,
                6,
            )?,
            7,
        )?;
        client.receive(&ServerFrame::SyncDone.encode(), 8)?;
        assert!(client.conversations()[0].can_send);
        assert!(client.conversations()[0].messages.is_empty());
        drop(native);
        Ok(Self {
            sqlite,
            native_key,
            author,
            client,
            log,
        })
    }

    fn retained_tail(&mut self) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error>> {
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
        )?;
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
            self.author
                .cert
                .as_deref()
                .ok_or("missing authenticated test as_deref value")?,
        )?;
        let ciphertext = native.seal(SPACE, &plaintext)?;
        let (content, sig) = self.author.sign_sealed(
            SPACE,
            &inner.client_id,
            9,
            inner.seen.as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, ciphertext),
        );
        sequence(
            &mut self.log,
            Envelope::new(content, sig, self.author.cert.clone(), None)
                .ok_or("authenticated test cryptographic evidence missing")?,
        )
    }

    fn assert_tail(&self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(self.client.logs[SPACE].next_seq(), self.log.next_seq());
        let views = self.client.conversations();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].messages.len(), 1);
        assert_eq!(views[0].messages[0].id, "final-retained-message");
        assert_eq!(views[0].messages[0].text, TEXT);
        assert_eq!(views[0].messages[0].seq.map(|sequence| sequence.0), Some(3));
        assert_eq!(views[0].messages[0].delivery, "confirmed");
        SpaceLog::from_events(SPACE, self.client.logs[SPACE].events().to_vec())?;

        Ok(())
    }
}

#[test]
fn retained_native_mls_tail_fixture_is_verified_exactly_once_and_survives_cold_restore(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let tail = f.retained_tail()?;
    f.client.receive(&tail, 10)?;
    f.assert_tail()?;
    let update = f.client.receive(&tail, 11)?;
    f.assert_tail()?;
    let account = f.client.account.clone();
    let signer = f.client.signer.clone();
    f.client = Client::new(account, signer, &update.snapshot)?;
    f.assert_tail()?;
    Ok(())
}

#[test]
fn forged_retained_tail_never_advances_the_browser_cursor(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let tail = f.retained_tail()?;
    let ServerFrame::Event { mut ev } =
        ServerFrame::decode(&tail).map_err(|error| error.to_string())?
    else {
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
    Ok(())
}

#[test]
fn healthy_browser_requests_the_unannounced_retained_final_event(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let tail = f.retained_tail()?;
    let next = f.client.logs[SPACE].next_seq();
    assert_eq!(next, 3);
    // No new Event, Joined, control notice, Pong or reconnect reaches this client.
    // A separate monotonic tick admits the repair, without UI/crypto maintenance.
    let crate::CursorRepairTick::Update { update } =
        f.client.reconcile(Duration::from_millis(60_008))?
    else {
        panic!("the scheduled repair must fit the original deadline")
    };
    let cursors = update
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .find_map(|frame| match frame {
            ClientFrame::Sync { cursors, all: true } => Some(cursors),
            _ => None,
        })
        .ok_or("a healthy browser must reconcile without a later live wake-up")?;
    assert_eq!(
        cursors,
        vec![Cursor {
            space: SPACE.into(),
            next_seq: next
        }]
    );
    assert_eq!(
        update
            .cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .abort_deadline_ms,
        Some(90_000)
    );
    f.client
        .receive_at(&tail, 10, Duration::from_millis(60_009))?;
    let saved = f.client.receive_at(
        &ServerFrame::SyncDone.encode(),
        11,
        Duration::from_millis(60_010),
    )?;
    f.assert_tail()?;
    f.client = Client::new(
        f.client.account.clone(),
        f.client.signer.clone(),
        &saved.snapshot,
    )?;
    f.assert_tail()?;
    let cold = f.client.bootstrap_at(Duration::from_millis(100))?;
    let sync = cold
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .find_map(|b| match b {
            ClientFrame::Sync { cursors, all: true } => Some(cursors),
            _ => None,
        })
        .ok_or("missing authenticated test find_map value")?;
    assert_eq!(
        sync,
        vec![Cursor {
            space: SPACE.into(),
            next_seq: 4
        }]
    );
    f.client.receive_at(&tail, 12, Duration::from_millis(101))?;
    f.client.receive_at(
        &ServerFrame::SyncDone.encode(),
        13,
        Duration::from_millis(102),
    )?;
    f.assert_tail()?;
    Ok(())
}

#[test]
fn ordered_browser_partials_and_pong_cannot_finish_or_postpone_a_full_repair(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let clock = |ms| Duration::from_millis(ms);
    f.client.receive_at(
        &ServerFrame::Joined {
            space: SPACE.into(),
        }
        .encode(),
        9,
        clock(10),
    )?;
    let crate::CursorRepairTick::Update { update } = f.client.reconcile(clock(60_008))? else {
        panic!("repair must be admitted")
    };
    assert_eq!(
        update
            .cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .abort_deadline_ms,
        Some(90_000)
    );
    f.client.receive_at(
        &ServerFrame::Joined {
            space: SPACE.into(),
        }
        .encode(),
        10,
        clock(60_009),
    )?;
    let pong = f.client.receive_at(
        &ServerFrame::Pong {
            server_time_ms: None,
        }
        .encode(),
        11,
        clock(60_010),
    )?;
    assert_eq!(
        pong.cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .abort_deadline_ms,
        Some(90_000)
    );
    let partial = f
        .client
        .receive_at(&ServerFrame::SyncDone.encode(), 12, clock(60_011))?;
    assert_eq!(
        partial
            .cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .abort_deadline_ms,
        Some(90_000)
    );
    let full = f
        .client
        .receive_at(&ServerFrame::SyncDone.encode(), 13, clock(60_012))?;
    assert_eq!(
        full.cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .admission_ms,
        Some(120_012)
    );
    let later = f
        .client
        .receive_at(&ServerFrame::SyncDone.encode(), 14, clock(65_000))?;
    assert_eq!(
        later
            .cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .admission_ms,
        Some(120_012)
    );
    let unsolicited = f
        .client
        .receive_at(&ServerFrame::SyncDone.encode(), 15, clock(70_000))?;
    assert_eq!(
        unsolicited
            .cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .admission_ms,
        Some(120_012)
    );
    Ok(())
}

#[test]
fn initial_verified_cursor_progress_renews_only_before_expiry_and_pong_cannot_do_so(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    f.client.bootstrap_at(Duration::ZERO)?;
    let tail = f.retained_tail()?;
    let progressed = f.client.receive_at(&tail, 10, Duration::from_secs(59))?;
    assert_eq!(
        progressed
            .cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .abort_deadline_ms,
        Some(119_000)
    );
    let duplicate = f.client.receive_at(&tail, 11, Duration::from_secs(118))?;
    assert_eq!(
        duplicate
            .cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .abort_deadline_ms,
        Some(119_000)
    );
    let pong = f.client.receive_at(
        &ServerFrame::Pong {
            server_time_ms: None,
        }
        .encode(),
        12,
        Duration::from_millis(118_999),
    )?;
    assert_eq!(
        pong.cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .abort_deadline_ms,
        Some(119_000)
    );
    assert!(matches!(
        f.client.reconcile(Duration::from_secs(119))?,
        crate::CursorRepairTick::Expired
    ));
    assert!(matches!(
        f.client.reconcile(Duration::from_millis(119_001))?,
        crate::CursorRepairTick::Expired
    ));
    f.assert_tail()?;
    Ok(())
}

#[test]
fn elapsed_or_backwards_clock_cannot_emit_late_sync_or_revive_a_prior_socket(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    assert!(matches!(
        f.client.reconcile(Duration::from_secs(90))?,
        crate::CursorRepairTick::Expired
    ));
    assert!(f.client.reconcile(Duration::from_millis(89_999)).is_err());
    let snapshot = f.client.update(Vec::new())?.snapshot;
    f.client = Client::new(f.client.account.clone(), f.client.signer.clone(), &snapshot)?;
    assert!(
        f.client.reconcile(Duration::ZERO).is_err(),
        "cold state has no socket owner"
    );
    let fresh = f.client.bootstrap_at(Duration::from_millis(7))?;
    assert_eq!(
        fresh
            .cursor_repair
            .ok_or("missing authenticated test cursor_repair")?
            .abort_deadline_ms,
        Some(60_007)
    );
    let sync = fresh
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .find_map(|b| match b {
            ClientFrame::Sync { cursors, all: true } => Some(cursors),
            _ => None,
        })
        .ok_or("missing authenticated test find_map value")?;
    assert_eq!(
        sync,
        vec![Cursor {
            space: SPACE.into(),
            next_seq: 3
        }]
    );
    Ok(())
}

#[test]
fn invalid_browser_clock_cannot_reset_the_existing_repair_owner_or_snapshot(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let before = f.client.update(Vec::new())?;
    assert!(f.client.bootstrap_at(Duration::MAX).is_err());
    assert!(f.client.bootstrap_at(Duration::from_nanos(1)).is_err());
    assert!(f.client.reconcile(Duration::MAX).is_err());
    let after = f.client.update(Vec::new())?;
    assert_eq!(before.snapshot, after.snapshot);
    assert_eq!(before.cursor_repair, after.cursor_repair);
    let config = f.client.cursor_repair_config()?;
    assert_eq!(
        (
            config.interval_ms,
            config.timeout_ms,
            config.initial_inactivity_ms
        ),
        (60_000, 30_000, 60_000)
    );
    Ok(())
}

/// Export only this independently keyed, synthetic SQLite/MLS fixture for the
/// opt-in headless browser contract. The output is never a user/account vault.
#[test]
#[ignore = "requires an explicitly chosen new synthetic fixture output file"]
fn export_synthetic_cursor_fixture_for_real_browser_worker(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let path = std::env::var_os("ZOEN_BROWSER_CURSOR_FIXTURE_OUTPUT")
        .ok_or("an owned synthetic output path must be explicit")?;
    let mut f = Fixture::new()?;
    let snapshot = f.client.update(Vec::new())?.snapshot;
    let tail = f.retained_tail()?;
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
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec(&body)?)?;
    file.sync_all()?;
    Ok(())
}
