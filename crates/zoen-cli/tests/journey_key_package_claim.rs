//! Destructive claims survive actual socket/process loss, without spending a second
//! package. Faults live solely in a transparent test transport around the real relay.
mod common;

use client_core::{
    AccountDto, Client, ConnectionDto, CoreListener, EntryKind, PrivacyDto, SecretVault,
};
use common::claim_proxy::{Boundary, ClaimProxy, Observed};
use common::{now_ms, RawClient, World};
use event_log::{
    profile::{sign_agreement, AgreementKey},
    Author, Signer,
};
use protocol::{
    ClientFrame, KeyPackageRecord, Op, Reply, ServerFrame, KEY_PACKAGE_CLAIM_EXPIRED,
    KEY_PACKAGE_CLAIM_TTL_MS,
};
use sqlx::{Connection, PgConnection};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};
use types::{Identity, IdentityKind};

struct Vault(PathBuf);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(key)).ok()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        std::fs::create_dir_all(&self.0).is_ok() && std::fs::write(self.0.join(key), value).is_ok()
    }
    fn delete(&self, key: String) {
        let _ = std::fs::remove_file(self.0.join(key));
    }
}

struct Refusals(tokio::sync::mpsc::UnboundedSender<String>);
impl CoreListener for Refusals {
    fn on_change(&self, _: Vec<String>) {}
    fn on_ephemeral(&self, _: String, _: String, _: String, _: String) {}
    fn on_presence(&self, _: String, _: bool) {}
    fn on_connection(&self, _: ConnectionDto) {}
    fn on_error(&self, message: String) {
        let _ = self.0.send(message);
    }
    fn on_profile_changed(&self, _: String) {}
}

struct Device {
    core: Arc<Client>,
    account: AccountDto,
    home: PathBuf,
    vault: Arc<Vault>,
}
impl Device {
    fn new(w: &World, who: &str, relay: &str) -> Self {
        let home = w.dir.join(who);
        std::fs::create_dir_all(&home).unwrap();
        let vault = Arc::new(Vault(home.join("vault")));
        let core = Client::open(
            home.join("zoen.sqlite").to_string_lossy().into_owned(),
            "en-US".into(),
        )
        .unwrap();
        let account = core
            .create_account(who.into(), who.into(), relay.into(), vault.clone())
            .unwrap();
        Self {
            core,
            account,
            home,
            vault,
        }
    }
    async fn close_database(&mut self) {
        self.core.stop_sync();
        let placeholder = Client::open(":memory:".into(), "en-US".into()).unwrap();
        drop(std::mem::replace(&mut self.core, placeholder));
        // shutdown_background may briefly retain the inner Engine. Observe actual
        // ownership release rather than racing a second app against EXCLUSIVE SQLite.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        loop {
            let available =
                rusqlite::Connection::open(self.home.join("zoen.sqlite")).and_then(|db| {
                    db.busy_timeout(Duration::ZERO)?;
                    db.execute_batch("BEGIN IMMEDIATE; COMMIT;")
                });
            if available.is_ok() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "device database still owned: {available:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    async fn reopen(&mut self) {
        self.close_database().await;
        let core = Client::open(
            self.home.join("zoen.sqlite").to_string_lossy().into_owned(),
            "en-US".into(),
        )
        .unwrap();
        assert!(core.unlock(self.vault.clone()).unwrap());
        self.core = core;
    }
    async fn start(&self) {
        self.core.start_sync(None).unwrap();
        self.core.wait_until_settled(8000).await.unwrap();
    }
    fn author(&self) -> Author {
        let root: [u8; 32] = self
            .vault
            .load("zoen.identity.v1".into())
            .unwrap()
            .try_into()
            .unwrap();
        let device: [u8; 32] = self
            .vault
            .load("zoen.device.v1".into())
            .unwrap()
            .try_into()
            .unwrap();
        Author::device(&Signer::from_secret(&root), Signer::from_secret(&device))
    }
    async fn stock(&self, w: &World) -> i64 {
        w.count(&format!(
            "SELECT count(*) FROM key_packages WHERE identity = '{}' AND NOT last_resort",
            self.account.identity_id
        ))
        .await
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        self.core.stop_sync();
    }
}

async fn next_request(proxy: &mut ClaimProxy) -> (String, Vec<String>) {
    let Observed::Request { operation, targets } = proxy.next().await else {
        panic!("claim request")
    };
    (
        operation.expect("a persisted operation must reach the wire"),
        targets,
    )
}

async fn next_reply(proxy: &mut ClaimProxy) -> (String, Vec<KeyPackageRecord>, bool) {
    let Observed::Reply {
        operation,
        records,
        held,
    } = proxy.next().await
    else {
        panic!("claim reply")
    };
    (operation.expect("claim operation"), records, held)
}

async fn readable(w: &World, ana: &Device, bruno: &Device, space: &str) {
    bruno.start().await;
    assert_eq!(
        ana.core.group_keys(space.into()),
        bruno.core.group_keys(space.into())
    );
    assert!(ana.core.group_keys(space.into()).is_some());
    let text = format!(
        "after the lost claim reply: {}",
        ana.core.timeline(space.into()).unwrap().len()
    );
    ana.core.send_message(space.into(), text.clone()).unwrap();
    ana.core.wait_until_settled(8000).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let entries = bruno.core.timeline(space.into()).unwrap_or_default();
        if entries
            .iter()
            .any(|e| matches!(&e.kind, EntryKind::Message { text: actual, .. } if actual == &text))
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "MLS message never opened\n{}",
            w.relay_log_text()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// These cohesive scenario files expand in this integration-test scope to keep
// every existing test name and share the actual Device/transport fixtures.
include!("key_package_claim/transport.rs");
include!("key_package_claim/receipts.rs");
