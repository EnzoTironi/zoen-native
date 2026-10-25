mod recovery;

use super::*;
use event_log::{device_cert_message, Signer};
use rusqlite::Connection;

const SPACE: &str = "space-e2e";

struct Person {
    identity: Signer,
    device: Signer,
    key: [u8; 32],
}

impl Person {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            identity: Signer::generate()?,
            device: Signer::generate()?,
            key: Signer::generate()?.secret(),
        })
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
fn pair(enzo: &Device, marina: &Device, both: &BTreeSet<String>) -> Vec<u8> {
    enzo.create_group(SPACE).unwrap();
    let c = add(enzo, marina);
    assert_eq!(
        enzo.open(SPACE, &c.commit, both, enzo.leaf()).unwrap(),
        Opened::Commit { epoch: 1 }
    );
    assert_eq!(
        marina.open(SPACE, &c.commit, both, enzo.leaf()).unwrap(),
        Opened::NotMember
    );
    assert!(marina.join(SPACE, &c.welcome.unwrap(), both).unwrap());
    c.group_info
}

#[test]
fn committed_leaves_wait_for_the_commit_and_follow_confirmed_membership(
) -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
    let (ce, cm) = (db(), db());
    let (enzo, marina) = (e.open(&ce), m.open(&cm));
    let both = roster(&[&e, &m]);
    assert_eq!(enzo.group_state(SPACE).unwrap(), GroupState::Missing);
    assert!(enzo.committed_leaves(SPACE).unwrap().is_none());
    enzo.create_group(SPACE).unwrap();
    assert_eq!(enzo.group_state(SPACE).unwrap(), GroupState::Ready);
    assert_eq!(
        enzo.committed_leaves(SPACE)
            .unwrap()
            .unwrap()
            .into_keys()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([(e.id(), e.device.id())])
    );
    let added = add(&enzo, &marina);
    assert_eq!(enzo.group_state(SPACE).unwrap(), GroupState::Pending);
    assert!(enzo.committed_leaves(SPACE).unwrap().is_none());
    assert_eq!(
        enzo.open(SPACE, &added.commit, &both, enzo.leaf()).unwrap(),
        Opened::Commit { epoch: 1 }
    );
    assert_eq!(enzo.group_state(SPACE).unwrap(), GroupState::Ready);
    let committed = enzo.committed_leaves(SPACE).unwrap().unwrap();
    assert_eq!(committed, enzo.leaves(SPACE).unwrap());
    assert_eq!(
        committed.keys().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from([(e.id(), e.device.id()), (m.id(), m.device.id())])
    );
    assert!(marina.join(SPACE, &added.welcome.unwrap(), &both).unwrap());
    let message = enzo.seal(SPACE, b"after the confirmed add").unwrap();
    assert!(
        matches!(marina.open(SPACE, &message, &both, enzo.leaf()).unwrap(),
                     Opened::Application { plaintext, .. } if plaintext == b"after the confirmed add")
    );
    let removed = enzo.commit(SPACE, &[], &BTreeSet::from([m.id()])).unwrap();
    assert_eq!(enzo.group_state(SPACE).unwrap(), GroupState::Pending);
    assert!(enzo.committed_leaves(SPACE).unwrap().is_none());
    assert_eq!(
        enzo.open(SPACE, &removed.commit, &roster(&[&e]), enzo.leaf())
            .unwrap(),
        Opened::Commit { epoch: 2 }
    );
    assert_eq!(enzo.group_state(SPACE).unwrap(), GroupState::Ready);
    assert_eq!(
        enzo.committed_leaves(SPACE)
            .unwrap()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([(e.id(), e.device.id())])
    );
    assert_eq!(
        committed.len(),
        2,
        "an inspection does not cache later mutations"
    );
    enzo.forget(SPACE).unwrap();
    assert_eq!(enzo.group_state(SPACE).unwrap(), GroupState::Missing);
    Ok(())
}

#[test]
fn provider_read_and_processing_failures_remain_storage_errors_and_allow_retry(
) -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
    let (ce, cm) = (db(), db());
    let (enzo, marina) = (e.open(&ce), m.open(&cm));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    let before = marina.checkpoint(SPACE).unwrap();
    let bytes = enzo.seal(SPACE, b"retry the same application").unwrap();
    cm.execute_batch("ALTER TABLE openmls_group_data RENAME TO unavailable_group_data")
        .unwrap();
    assert!(matches!(
        marina.group_state(SPACE),
        Err(MlsError::Storage(_))
    ));
    assert!(matches!(
        marina.committed_leaves(SPACE),
        Err(MlsError::Storage(_))
    ));
    assert!(matches!(
        marina.open(SPACE, &bytes, &both, enzo.leaf()),
        Err(MlsError::Storage(_))
    ));
    cm.execute_batch("ALTER TABLE unavailable_group_data RENAME TO openmls_group_data")
        .unwrap();
    assert_eq!(marina.group_state(SPACE).unwrap(), GroupState::Ready);
    assert_eq!(
        marina.committed_leaves(SPACE).unwrap().unwrap(),
        marina.leaves(SPACE).unwrap()
    );
    assert_eq!(marina.checkpoint(SPACE).unwrap(), before);
    cm.execute_batch(
        "CREATE TRIGGER fail_processing BEFORE INSERT ON openmls_group_data
         WHEN NEW.data_type = 'message_secrets'
         BEGIN SELECT RAISE(ABORT, 'injected processing failure'); END;",
    )
    .unwrap();
    assert!(matches!(
        marina.open(SPACE, &bytes, &both, enzo.leaf()),
        Err(MlsError::Storage(ref reason)) if reason.contains("injected processing failure")
    ));
    cm.execute_batch("DROP TRIGGER fail_processing").unwrap();
    assert_eq!(marina.checkpoint(SPACE).unwrap(), before);
    assert!(matches!(
        marina.open(SPACE, &bytes, &both, enzo.leaf()).unwrap(),
        Opened::Application { plaintext, .. } if plaintext == b"retry the same application"
    ));
    Ok(())
}

#[test]
fn external_commit_cannot_add_an_unlisted_identity_or_impersonate_an_admitted_device(
) -> Result<(), Box<dyn std::error::Error>> {
    let (e, m, outsider) = (Person::new()?, Person::new()?, Person::new()?);
    let (ce, cm, cx) = (db(), db(), db());
    let (enzo, marina, intruder) = (e.open(&ce), m.open(&cm), outsider.open(&cx));
    let both = roster(&[&e, &m]);
    let context = pair(&enzo, &marina, &both);
    let before = (
        marina.epoch(SPACE).unwrap(),
        marina.leaves(SPACE).unwrap(),
        marina.checkpoint(SPACE).unwrap(),
    );
    // A malicious client can construct a valid external commit with its own
    // roster argument, then ask an active member to wrap it in an admitted frame.
    let external = intruder
        .recover(SPACE, &context, &roster(&[&e, &m, &outsider]))
        .unwrap();
    assert!(
        matches!(marina.open(SPACE, &external.commit, &both, enzo.leaf()), Err(MlsError::Unlisted(ids)) if ids.contains(&outsider.id()))
    );
    assert_eq!(
        (
            marina.epoch(SPACE).unwrap(),
            marina.leaves(SPACE).unwrap(),
            marina.checkpoint(SPACE).unwrap()
        ),
        before
    );

    // A root-certified device still needs its own enrollment/admission. A
    // revoked or un-enrolled same-identity device cannot borrow Enzo's wrapper.
    let forged_device = Person {
        identity: Signer::from_secret(&e.identity.secret()),
        device: Signer::generate()?,
        key: Signer::generate()?.secret(),
    };
    let forged_db = db();
    let forged = forged_device.open(&forged_db);
    let external = forged.recover(SPACE, &context, &both).unwrap();
    assert!(matches!(
        marina.open(SPACE, &external.commit, &both, enzo.leaf()),
        Err(MlsError::Credential)
    ));
    assert_eq!(
        (
            marina.epoch(SPACE).unwrap(),
            marina.leaves(SPACE).unwrap(),
            marina.checkpoint(SPACE).unwrap()
        ),
        before
    );
    let fresh = Person {
        identity: Signer::from_secret(&e.identity.secret()),
        device: Signer::generate()?,
        key: Signer::generate()?.secret(),
    };
    let c = db();
    let admitted = fresh.open(&c);
    let external = admitted.recover(SPACE, &context, &both).unwrap();
    assert_eq!(
        marina
            .open(SPACE, &external.commit, &both, admitted.leaf())
            .unwrap(),
        Opened::Commit { epoch: 2 }
    );
    admitted
        .confirm_recovery(SPACE, &external.commit, &both)
        .unwrap();
    let bytes = admitted.seal(SPACE, b"freshly admitted recovery").unwrap();
    assert!(
        matches!(marina.open(SPACE, &bytes, &both, admitted.leaf()).unwrap(), Opened::Application { plaintext, .. } if plaintext == b"freshly admitted recovery")
    );
    Ok(())
}

#[test]
fn two_devices_talk_both_ways() -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
    let (ce, cm) = (db(), db());
    let (enzo, marina) = (e.open(&ce), m.open(&cm));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    assert_eq!(marina.epoch(SPACE).unwrap(), 1);
    assert_eq!(marina.roster(SPACE).unwrap(), both);

    let hi = enzo.seal(SPACE, b"oi Marina").unwrap();
    assert_eq!(
        enzo.open(SPACE, &hi, &both, enzo.leaf()).unwrap(),
        Opened::Own
    );
    let Opened::Application { plaintext, from } =
        marina.open(SPACE, &hi, &both, enzo.leaf()).unwrap()
    else {
        panic!("expected an application message");
    };
    assert_eq!(
        (plaintext.as_slice(), from.identity.as_str()),
        (&b"oi Marina"[..], e.id().as_str())
    );

    let back = marina.seal(SPACE, b"oi Enzo").unwrap();
    assert!(
        matches!(enzo.open(SPACE, &back, &both, marina.leaf()).unwrap(),
        Opened::Application { plaintext, .. } if plaintext == b"oi Enzo")
    );
    assert_eq!(
        enzo.checkpoint(SPACE).unwrap(),
        marina.checkpoint(SPACE).unwrap()
    );
    Ok(())
}

#[test]
fn a_commit_adding_someone_the_log_doesnt_list_is_refused() -> Result<(), Box<dyn std::error::Error>>
{
    let (e, m, b) = (Person::new()?, Person::new()?, Person::new()?);
    let (ce, cm, cb) = (db(), db(), db());
    let (enzo, marina, bruno) = (e.open(&ce), m.open(&cm), b.open(&cb));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    // Enzo adds Bruno, but the log still lists two people.
    let c = add(&enzo, &bruno);
    assert!(enzo.pending(SPACE));
    for d in [&marina, &enzo] {
        assert!(
            matches!(d.open(SPACE, &c.commit, &both, enzo.leaf()), Err(MlsError::Unlisted(x)) if x.contains(&b.id()))
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
            d.open(SPACE, &again.commit, &three, enzo.leaf()).unwrap(),
            Opened::Commit { epoch: 2 }
        );
    }
    assert!(bruno.join(SPACE, &again.welcome.unwrap(), &three).unwrap());
    assert_eq!(
        bruno.checkpoint(SPACE).unwrap(),
        marina.checkpoint(SPACE).unwrap()
    );
    Ok(())
}

#[test]
fn a_listed_person_the_group_lacks_yet_doesnt_block_a_commit(
) -> Result<(), Box<dyn std::error::Error>> {
    let (e, m, b) = (Person::new()?, Person::new()?, Person::new()?);
    let (ce, cm, cb) = (db(), db(), db());
    let (enzo, marina, bruno) = (e.open(&ce), m.open(&cm), b.open(&cb));
    enzo.create_group(SPACE).unwrap();
    // The log already lists all three; this commit adds only Marina.
    let three = roster(&[&e, &m, &b]);
    let c = add(&enzo, &marina);
    assert_eq!(
        enzo.open(SPACE, &c.commit, &three, enzo.leaf()).unwrap(),
        Opened::Commit { epoch: 1 }
    );
    assert!(marina.join(SPACE, &c.welcome.unwrap(), &three).unwrap());
    let next = add(&enzo, &bruno);
    for d in [&enzo, &marina] {
        assert_eq!(
            d.open(SPACE, &next.commit, &three, enzo.leaf()).unwrap(),
            Opened::Commit { epoch: 2 }
        );
    }
    assert!(bruno.join(SPACE, &next.welcome.unwrap(), &three).unwrap());
    Ok(())
}

#[test]
fn a_removed_member_reads_nothing_after_the_commit() -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
    let (ce, cm) = (db(), db());
    let (enzo, marina) = (e.open(&ce), m.open(&cm));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    let alone = roster(&[&e]);
    let c = enzo.commit(SPACE, &[], &roster(&[&m])).unwrap();
    assert!(c.welcome.is_none());
    for d in [&enzo, &marina] {
        assert_eq!(
            d.open(SPACE, &c.commit, &alone, enzo.leaf()).unwrap(),
            Opened::Commit { epoch: 2 }
        );
    }
    assert_eq!(enzo.roster(SPACE).unwrap(), alone);
    let secret = enzo.seal(SPACE, b"so pra mim").unwrap();
    assert_eq!(
        marina.open(SPACE, &secret, &alone, enzo.leaf()).unwrap(),
        Opened::NotMember
    );
    Ok(())
}

#[test]
fn a_removed_device_forgets_the_group_and_can_be_added_back(
) -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
    let (ce, cm) = (db(), db());
    let (enzo, marina) = (e.open(&ce), m.open(&cm));
    let both = roster(&[&e, &m]);
    pair(&enzo, &marina, &both);
    let c = enzo.commit(SPACE, &[], &roster(&[&m])).unwrap();
    enzo.open(SPACE, &c.commit, &roster(&[&e]), enzo.leaf())
        .unwrap();
    // Marina's device never sees that commit (the relay stops sending to her): it forgets.
    marina.forget(SPACE).unwrap();
    assert!(!marina.has_group(SPACE));
    let back = add(&enzo, &marina);
    enzo.open(SPACE, &back.commit, &both, enzo.leaf()).unwrap();
    assert!(marina.join(SPACE, &back.welcome.unwrap(), &both).unwrap());
    let hi = enzo.seal(SPACE, b"de volta").unwrap();
    assert!(matches!(
        marina.open(SPACE, &hi, &both, enzo.leaf()).unwrap(),
        Opened::Application { plaintext, .. } if plaintext == b"de volta"
    ));
    Ok(())
}

#[test]
fn the_first_commit_wins_and_the_other_is_stale() -> Result<(), Box<dyn std::error::Error>> {
    let (e, m, b, j) = (
        Person::new()?,
        Person::new()?,
        Person::new()?,
        Person::new()?,
    );
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
            d.open(SPACE, &first.commit, &three, marina.leaf()).unwrap(),
            Opened::Commit { epoch: 2 }
        );
        assert_eq!(
            d.open(SPACE, &second.commit, &three, enzo.leaf()).unwrap(),
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
            d.open(SPACE, &again.commit, &four, enzo.leaf()).unwrap(),
            Opened::Commit { epoch: 3 }
        );
    }
    assert!(julia.join(SPACE, &again.welcome.unwrap(), &four).unwrap());
    let hi = enzo.seal(SPACE, b"bem-vinda").unwrap();
    assert!(matches!(
        julia.open(SPACE, &hi, &four, enzo.leaf()).unwrap(),
        Opened::Application { plaintext, .. } if plaintext == b"bem-vinda"
    ));
    Ok(())
}

#[test]
fn a_forged_device_cannot_be_added() -> Result<(), Box<dyn std::error::Error>> {
    let (e, m, mallory) = (Person::new()?, Person::new()?, Person::new()?);
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
    Ok(())
}

#[test]
fn nothing_is_stored_in_the_clear() -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
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
        ..Person::new()?
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
    Ok(())
}
