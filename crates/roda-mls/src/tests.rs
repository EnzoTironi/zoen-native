use super::*;
use roda_log::{device_cert_message, Signer};
use rusqlite::Connection;

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

fn add(by: &Device, joiner: &Device) -> Commit {
    by.commit(
        SPACE,
        &joiner.key_packages(1, false).unwrap(),
        &BTreeSet::new(),
    )
    .unwrap()
}

/// Enzo creates the group and adds Marina; both have applied the commit.
fn pair(enzo: &Device, marina: &Device, both: &BTreeSet<String>) {
    enzo.create_group(SPACE).unwrap();
    let c = add(enzo, marina);
    assert_eq!(
        enzo.open(SPACE, &c.commit, both).unwrap(),
        Opened::Commit { epoch: 1 }
    );
    assert_eq!(
        marina.open(SPACE, &c.commit, both).unwrap(),
        Opened::NotMember
    );
    assert!(marina.join(SPACE, &c.welcome.unwrap(), both).unwrap());
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
fn a_commit_adding_someone_the_log_doesnt_list_is_refused() {
    let (e, m, b) = (Person::new(), Person::new(), Person::new());
    let (ce, cm, cb) = (db(), db(), db());
    let (enzo, marina, bruno) = (e.open(&ce), m.open(&cm), b.open(&cb));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    // Enzo adds Bruno, but the log still lists two people.
    let c = add(&enzo, &bruno);
    assert!(enzo.pending(SPACE));
    for d in [&marina, &enzo] {
        assert!(
            matches!(d.open(SPACE, &c.commit, &both), Err(MlsError::Unlisted(x)) if x.contains(&b.id()))
        );
        assert_eq!(d.epoch(SPACE).unwrap(), 1);
    }
    // The refused commit is dropped, so Enzo can commit again once the log lists Bruno.
    assert!(!enzo.pending(SPACE));
    assert!(matches!(
        bruno.join(SPACE, &c.welcome.unwrap(), &both),
        Err(MlsError::Unlisted(_))
    ));
    assert!(!bruno.has_group(SPACE), "and Bruno never joined");
    let three = roster(&[&e, &m, &b]);
    let again = add(&enzo, &bruno);
    for d in [&enzo, &marina] {
        assert_eq!(
            d.open(SPACE, &again.commit, &three).unwrap(),
            Opened::Commit { epoch: 2 }
        );
    }
    assert!(bruno.join(SPACE, &again.welcome.unwrap(), &three).unwrap());
    assert_eq!(
        bruno.checkpoint(SPACE).unwrap(),
        marina.checkpoint(SPACE).unwrap()
    );
}

#[test]
fn a_listed_person_the_group_lacks_yet_doesnt_block_a_commit() {
    let (e, m, b) = (Person::new(), Person::new(), Person::new());
    let (ce, cm, cb) = (db(), db(), db());
    let (enzo, marina, bruno) = (e.open(&ce), m.open(&cm), b.open(&cb));
    enzo.create_group(SPACE).unwrap();
    // The log already lists all three; this commit adds only Marina.
    let three = roster(&[&e, &m, &b]);
    let c = add(&enzo, &marina);
    assert_eq!(
        enzo.open(SPACE, &c.commit, &three).unwrap(),
        Opened::Commit { epoch: 1 }
    );
    assert!(marina.join(SPACE, &c.welcome.unwrap(), &three).unwrap());
    let next = add(&enzo, &bruno);
    for d in [&enzo, &marina] {
        assert_eq!(
            d.open(SPACE, &next.commit, &three).unwrap(),
            Opened::Commit { epoch: 2 }
        );
    }
    assert!(bruno.join(SPACE, &next.welcome.unwrap(), &three).unwrap());
}

#[test]
fn a_removed_member_reads_nothing_after_the_commit() {
    let (e, m) = (Person::new(), Person::new());
    let (ce, cm) = (db(), db());
    let (enzo, marina) = (e.open(&ce), m.open(&cm));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    let alone = roster(&[&e]);
    let c = enzo.commit(SPACE, &[], &roster(&[&m])).unwrap();
    assert!(c.welcome.is_none());
    for d in [&enzo, &marina] {
        assert_eq!(
            d.open(SPACE, &c.commit, &alone).unwrap(),
            Opened::Commit { epoch: 2 }
        );
    }
    assert_eq!(enzo.roster(SPACE).unwrap(), alone);
    let secret = enzo.seal(SPACE, b"so pra mim").unwrap();
    assert_eq!(
        marina.open(SPACE, &secret, &alone).unwrap(),
        Opened::NotMember
    );
}

#[test]
fn a_removed_device_forgets_the_group_and_can_be_added_back() {
    let (e, m) = (Person::new(), Person::new());
    let (ce, cm) = (db(), db());
    let (enzo, marina) = (e.open(&ce), m.open(&cm));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    let c = enzo.commit(SPACE, &[], &roster(&[&m])).unwrap();
    enzo.open(SPACE, &c.commit, &roster(&[&e])).unwrap();
    // Marina's device never sees that commit (the relay stops sending to her): it forgets.
    marina.forget(SPACE).unwrap();
    assert!(!marina.has_group(SPACE));
    let back = add(&enzo, &marina);
    enzo.open(SPACE, &back.commit, &both).unwrap();
    assert!(marina.join(SPACE, &back.welcome.unwrap(), &both).unwrap());
    let hi = enzo.seal(SPACE, b"de volta").unwrap();
    assert!(matches!(
        marina.open(SPACE, &hi, &both).unwrap(),
        Opened::Application { plaintext, .. } if plaintext == b"de volta"
    ));
}

#[test]
fn the_first_commit_wins_and_the_other_is_stale() {
    let (e, m, b, j) = (Person::new(), Person::new(), Person::new(), Person::new());
    let (ce, cm, cb, cj) = (db(), db(), db(), db());
    let (enzo, marina, bruno, julia) = (e.open(&ce), m.open(&cm), b.open(&cb), j.open(&cj));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    let first = add(&marina, &bruno);
    let second = add(&enzo, &julia);
    // Both were made at epoch 1: the relay sees that in the clear and keeps the first.
    assert_eq!(commit_epoch(&first.commit), Some(1));
    assert_eq!(commit_epoch(&second.commit), Some(1));
    assert_eq!(commit_epoch(&marina.seal(SPACE, b"oi").unwrap()), None);
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
    assert!(bruno.join(SPACE, &first.welcome.unwrap(), &three).unwrap());
    assert!(!julia
        .join(SPACE, &second.welcome.unwrap(), &three)
        .unwrap_or(false));
    assert_eq!(
        enzo.checkpoint(SPACE).unwrap(),
        bruno.checkpoint(SPACE).unwrap()
    );
    // The loser drops its commit and adds Julia again on top of the winner.
    enzo.abandon(SPACE).unwrap();
    assert!(!enzo.pending(SPACE));
    let again = add(&enzo, &julia);
    assert_eq!(commit_epoch(&again.commit), Some(2));
    let four = roster(&[&e, &m, &b, &j]);
    for d in [&enzo, &marina, &bruno] {
        assert_eq!(
            d.open(SPACE, &again.commit, &four).unwrap(),
            Opened::Commit { epoch: 3 }
        );
    }
    assert!(julia.join(SPACE, &again.welcome.unwrap(), &four).unwrap());
    let hi = enzo.seal(SPACE, b"bem-vinda").unwrap();
    assert!(matches!(
        julia.open(SPACE, &hi, &four).unwrap(),
        Opened::Application { plaintext, .. } if plaintext == b"bem-vinda"
    ));
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
    let honest = m.open(&cm).key_packages(1, true).unwrap();
    for kp in &honest {
        assert_eq!(key_package_leaf(kp).unwrap().identity, m.id());
    }
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
