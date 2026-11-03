//! Fallible setup for real MLS group and recovery fixtures.
use super::{Commit, Device, Opened, SPACE};
use crate::migrate;
use event_log::{device_cert_message, Signer};
use rusqlite::Connection;
use std::collections::BTreeSet;

pub(super) struct Person {
    pub(super) identity: Signer,
    pub(super) device: Signer,
    pub(super) key: [u8; 32],
}

impl Person {
    pub(super) fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            identity: Signer::generate()?,
            device: Signer::generate()?,
            key: Signer::generate()?.secret(),
        })
    }
    pub(super) fn id(&self) -> String {
        self.identity.id()
    }
    pub(super) fn open<'c>(
        &self,
        conn: &'c Connection,
    ) -> Result<Device<'c>, Box<dyn std::error::Error>> {
        let cert = self.identity.sign(&device_cert_message(&self.device.id()));
        Ok(Device::new(
            conn,
            self.key,
            &self.id(),
            self.device.secret(),
            &cert,
        )?)
    }
}

pub(super) fn db() -> Result<Connection, Box<dyn std::error::Error>> {
    let mut c = Connection::open_in_memory()?;
    migrate(&mut c)?;
    Ok(c)
}

pub(super) fn roster(people: &[&Person]) -> BTreeSet<String> {
    people.iter().map(|p| p.id()).collect()
}

pub(super) fn add(by: &Device, joiner: &Device) -> Result<Commit, Box<dyn std::error::Error>> {
    Ok(by.commit(SPACE, &joiner.key_packages(1, false)?, &BTreeSet::new())?)
}

/// Enzo creates the group and adds Marina; both have applied the commit.
pub(super) fn pair(
    enzo: &Device,
    marina: &Device,
    both: &BTreeSet<String>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    enzo.create_group(SPACE)?;
    let c = add(enzo, marina)?;
    assert_eq!(
        enzo.open(SPACE, &c.commit, both, enzo.leaf())?,
        Opened::Commit { epoch: 1 }
    );
    assert_eq!(
        marina.open(SPACE, &c.commit, both, enzo.leaf())?,
        Opened::NotMember
    );
    assert!(marina.join(
        SPACE,
        c.welcome.as_deref().ok_or("fixture welcome missing")?,
        both
    )?);
    Ok(c.group_info)
}
