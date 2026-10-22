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
        assert_eq!(views[0].messages[0].id, "msg_final_retained");
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
    let _tail = f.retained_tail();
    let next = f.client.logs[SPACE].next_seq();
    assert_eq!(next, 3);
    // No new Event, Joined, control notice, Pong or reconnect reaches this client.
    // The original maintenance entrypoint cannot discover the retained final row.
    let update = f.client.maintenance(60_008).unwrap();
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
}
