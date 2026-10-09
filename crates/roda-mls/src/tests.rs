use super::*;
use roda_log::{device_cert_message, Signer};

const SPACE: &str = "space-e2e";

struct Person {
    identity: Signer,
    device: Signer,
    key: [u8; 32],
}

impl Person {
    fn new() -> Self {
        Self {
            identity: Signer::generate(),
            device: Signer::generate(),
            key: Signer::generate().secret(),
        }
    }
    fn id(&self) -> String {
        self.identity.id()
    }
    fn open<'c>(&self, conn: &'c Connection) -> Device<'c> {
        let cert = self.identity.sign(&device_cert_message(&self.device.id()));
        Device::new(conn, self.key, &self.id(), self.device.secret(), &cert).unwrap()
    }
}

fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    migrate(&mut c).unwrap();
    c
}

fn roster(people: &[&Person]) -> BTreeSet<String> {
    people.iter().map(|p| p.id()).collect()
}

/// Enzo creates the group and adds Marina; both have applied the commit.
fn pair<'a>(enzo: &Device<'a>, marina: &Device<'a>, both: &BTreeSet<String>) -> Added {
    enzo.create_group(SPACE).unwrap();
    let kp = marina.key_packages(1, false).unwrap();
    let added = enzo.add(SPACE, &kp).unwrap();
    assert_eq!(
        enzo.open(SPACE, &added.commit, both).unwrap(),
        Opened::Commit { epoch: 1 }
    );
    assert_eq!(
        marina.open(SPACE, &added.commit, both).unwrap(),
        Opened::NotMember
    );
    assert!(marina.join(SPACE, &added.welcome, both).unwrap());
    added
}

#[test]
fn two_devices_talk_both_ways() {
    let (e, m) = (Person::new(), Person::new());
    let (ce, cm) = (db(), db());
    let (enzo, marina) = (e.open(&ce), m.open(&cm));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    assert_eq!(marina.epoch(SPACE).unwrap(), 1);
    assert_eq!(marina.roster(SPACE).unwrap(), both);

    let hi = enzo.seal(SPACE, b"oi Marina").unwrap();
    assert_eq!(enzo.open(SPACE, &hi, &both).unwrap(), Opened::Own);
    let Opened::Application { plaintext, from } = marina.open(SPACE, &hi, &both).unwrap() else {
        panic!("expected an application message");
    };
    assert_eq!(
        (plaintext.as_slice(), from.identity.as_str()),
        (&b"oi Marina"[..], e.id().as_str())
    );

    let back = marina.seal(SPACE, b"oi Enzo").unwrap();
    assert!(matches!(enzo.open(SPACE, &back, &both).unwrap(),
        Opened::Application { plaintext, .. } if plaintext == b"oi Enzo"));
    assert_eq!(
        enzo.checkpoint(SPACE).unwrap(),
        marina.checkpoint(SPACE).unwrap()
    );
}

#[test]
fn state_survives_reopen() {
    let (e, m) = (Person::new(), Person::new());
    let (ce, cm) = (db(), db());
    let both = roster(&[&e, &m]);
    pair(&e.open(&ce), &m.open(&cm), &both);
    let msg = e.open(&ce).seal(SPACE, b"after restart").unwrap();
    let marina = m.open(&cm);
    assert!(marina.has_group(SPACE));
    assert!(matches!(
        marina.open(SPACE, &msg, &both).unwrap(),
        Opened::Application { .. }
    ));
}

#[test]
fn a_commit_that_disagrees_with_the_log_is_refused() {
    let (e, m, b) = (Person::new(), Person::new(), Person::new());
    let (ce, cm, cb) = (db(), db(), db());
    let (enzo, marina, bruno) = (e.open(&ce), m.open(&cm), b.open(&cb));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    // Enzo adds Bruno, but the log (as Marina sees it) says the roster is still two.
    let added = enzo
        .add(SPACE, &bruno.key_packages(1, false).unwrap())
        .unwrap();
    assert!(matches!(
        marina.open(SPACE, &added.commit, &both),
        Err(MlsError::RosterMismatch { .. })
    ));
    assert_eq!(marina.epoch(SPACE).unwrap(), 1);
    // And a Welcome into a group other than the log's roster isn't joined. It fails
    // closed: the key package it was for is spent, so the same Welcome can't be retried.
    let three = roster(&[&e, &m, &b]);
    assert!(matches!(
        bruno.join(SPACE, &added.welcome, &both),
        Err(MlsError::RosterMismatch { .. })
    ));
    assert!(!bruno.has_group(SPACE));
    assert!(!bruno.join(SPACE, &added.welcome, &three).unwrap());
}

#[test]
fn the_first_commit_wins_and_the_other_is_stale() {
    let (e, m, b, j) = (Person::new(), Person::new(), Person::new(), Person::new());
    let (ce, cm, cb, cj) = (db(), db(), db(), db());
    let (enzo, marina, bruno, julia) = (e.open(&ce), m.open(&cm), b.open(&cb), j.open(&cj));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    let first = marina
        .add(SPACE, &bruno.key_packages(1, false).unwrap())
        .unwrap();
    let second = enzo
        .add(SPACE, &julia.key_packages(1, false).unwrap())
        .unwrap();
    let three = roster(&[&e, &m, &b]);
    for d in [&enzo, &marina] {
        assert_eq!(
            d.open(SPACE, &first.commit, &three).unwrap(),
            Opened::Commit { epoch: 2 }
        );
        assert_eq!(
            d.open(SPACE, &second.commit, &three).unwrap(),
            Opened::Stale
        );
    }
    assert!(bruno.join(SPACE, &first.welcome, &three).unwrap());
    assert!(!julia.join(SPACE, &second.welcome, &three).unwrap_or(false));
    assert_eq!(
        enzo.checkpoint(SPACE).unwrap(),
        bruno.checkpoint(SPACE).unwrap()
    );
}

#[test]
fn a_forged_device_cannot_be_added() {
    let (e, m, mallory) = (Person::new(), Person::new(), Person::new());
    let (ce, cm) = (db(), db());
    let enzo = e.open(&ce);
    enzo.create_group(SPACE).unwrap();
    // Mallory's device claims Marina's identity with a cert Mallory made.
    let forged = mallory
        .identity
        .sign(&device_cert_message(&mallory.device.id()));
    assert!(matches!(
        Device::new(&cm, m.key, &m.id(), mallory.device.secret(), &forged),
        Err(MlsError::Credential)
    ));
    let honest = m.open(&cm).key_packages(1, false).unwrap();
    assert_eq!(key_package_leaf(&honest[0]).unwrap().identity, m.id());
    assert!(key_package_leaf(b"garbage").is_err());
}

#[test]
fn nothing_is_stored_in_the_clear() {
    let (e, m) = (Person::new(), Person::new());
    let (ce, cm) = (db(), db());
    let both = roster(&[&e, &m]);
    pair(&e.open(&ce), &m.open(&cm), &both);
    let mut tables = ce
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .unwrap();
    let names: Vec<String> = tables
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let needle = e.device.id();
    let mut values = 0;
    for t in names {
        let Ok(mut q) = ce.prepare(&format!("SELECT * FROM \"{t}\"")) else {
            continue;
        };
        let cols = q.column_count();
        let mut rows = q.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..cols {
                if let Ok(rusqlite::types::ValueRef::Blob(b) | rusqlite::types::ValueRef::Text(b)) =
                    row.get_ref(i)
                {
                    values += 1;
                    let s = String::from_utf8_lossy(b);
                    assert!(
                        !s.contains(&needle) && !s.contains("zoen-leaf"),
                        "{t} holds plaintext"
                    );
                }
            }
        }
    }
    assert!(values > 0);
    // Under another key the state doesn't open.
    let other = Person {
        key: [7; 32],
        ..Person::new()
    };
    let wrong = Device::new(
        &ce,
        other.key,
        &e.id(),
        e.device.secret(),
        &e.identity.sign(&device_cert_message(&e.device.id())),
    )
    .unwrap();
    assert!(!wrong.has_group(SPACE));
}
