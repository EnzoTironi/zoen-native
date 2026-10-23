//! Offline account setup, before the World starts its first relay. We constrain
//! sketch inputs, not observed outputs: every activity/counter still comes from
//! the real relay, real signatures and fresh CLI processes.
use crate::common::World;
use client_core::Engine;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write, path::Path};

pub const PEOPLE: [(&str, &str); 5] = [
    ("ana", "Ana"),
    ("bruno", "Bruno"),
    ("carla", "Carla"),
    ("davi", "Davi"),
    ("qa_bot", "QA"),
];

fn secret(path: &Path, bytes: &[u8]) {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .expect("new fixture secret")
        .write_all(bytes)
        .expect("save fixture secret");
}

fn peer_slot(key: &[u8], identity: &str) -> u8 {
    Sha256::new()
        .chain_update(key)
        .chain_update(b"peer\0")
        .chain_update(identity.as_bytes())
        .finalize()[0]
        & 63
}

pub fn prepare(world: &World) -> Vec<(String, String)> {
    let identities: Vec<_> = PEOPLE
        .iter()
        .map(|(handle, name)| {
            let home = world.dir.join(handle);
            let vault = home.join("vault");
            let mut directories = fs::DirBuilder::new();
            directories.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                directories.mode(0o700);
            }
            directories.create(&vault).expect("private fixture home");
            let mut core = Engine::open(&home.join("zoen.sqlite").to_string_lossy())
                .expect("offline fixture core");
            let (root, device, agreement) = core
                .create_account(name, handle, &world.relay_url())
                .expect("prepare real person account");
            for (key, bytes) in [
                ("zoen.identity.v1", root),
                ("zoen.device.v1", device),
                ("zoen.agreement.v1", agreement),
            ] {
                secret(&vault.join(key), &bytes);
            }
            let account = core.account().expect("offline fixture account");
            assert!(
                !account.registered,
                "fixture must not fake relay registration"
            );
            account.identity.clone()
        })
        .collect();
    // Public person creation deliberately generates fresh roots. Choose our test
    // salt from those offline identities, before any pseudonym/event is recorded.
    // This bounded fixture search is not a retry of a command or failed assertion.
    for candidate in 0..4096 {
        let salt = format!("metrics-positive-fixture-{candidate}");
        let key = Sha256::digest(salt.as_bytes());
        let slots: BTreeSet<_> = identities.iter().map(|id| peer_slot(&key, id)).collect();
        if slots.len() == identities.len() {
            return vec![("ZOEN_METRICS_KEY".into(), salt)];
        }
    }
    panic!("could not construct distinct peer slots before relay startup");
}
