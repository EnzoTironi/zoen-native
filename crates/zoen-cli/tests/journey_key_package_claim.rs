//! Destructive claims survive actual socket/process loss, without spending a second
//! package. Faults live solely in a transparent test transport around the real relay.
#[path = "common/claim_proxy.rs"]
mod claim_proxy;
mod common;

use claim_proxy::{Boundary, ClaimProxy, Observed};
use common::{now_ms, RawClient, World};
use roda_ffi::{AccountDto, EntryKind, PrivacyDto, RodaEngine, SecretVault};
use roda_log::{Author, Signer};
use roda_proto::{
    KeyPackageRecord, Op, Reply, ServerFrame, KEY_PACKAGE_CLAIM_EXPIRED, KEY_PACKAGE_CLAIM_TTL_MS,
};
use sqlx::{Connection, PgConnection};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};

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

struct Device {
    core: Arc<RodaEngine>,
    account: AccountDto,
    home: PathBuf,
    vault: Arc<Vault>,
}
impl Device {
    fn new(w: &World, who: &str, relay: &str) -> Self {
        let home = w.dir.join(who);
        std::fs::create_dir_all(&home).unwrap();
        let vault = Arc::new(Vault(home.join("vault")));
        let core = RodaEngine::open(
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
    fn reopen(&mut self) {
        self.core.stop_sync();
        let core = RodaEngine::open(
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
    ana.core
        .send_message(space.into(), "after the lost claim reply".into())
        .unwrap();
    ana.core.wait_until_settled(8000).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let entries = bruno.core.timeline(space.into()).unwrap_or_default();
        if entries.iter().any(|e| matches!(&e.kind, EntryKind::Message { text, .. } if text == "after the lost claim reply")) { break }
        assert!(
            tokio::time::Instant::now() < deadline,
            "MLS message never opened\n{}",
            w.relay_log_text()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_claim_reply_lost_across_restart_spends_exactly_one_package() {
    let w = World::new("kp_claim_loss").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    assert_eq!(bruno.stock(&w).await, 32);
    bruno.core.stop_sync();
    let mut proxy = ClaimProxy::new(w.port, Boundary::Reply).await;
    let mut ana = Device::new(&w, "ana", &proxy.url);
    ana.start().await;
    let space = ana
        .core
        .create_group_with(
            "Lost reply".into(),
            vec![bruno.account.identity_id.clone()],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    let (operation, targets) = next_request(&mut proxy).await;
    assert_eq!(targets, vec![bruno.account.identity_id.clone()]);
    let (reply_operation, first, held) = next_reply(&mut proxy).await;
    assert_eq!(reply_operation, operation);
    assert!(held, "the first real committed reply must be withheld");
    assert_eq!(first.len(), 1);
    assert_eq!(bruno.stock(&w).await, 31);
    assert_eq!(
        w.count("SELECT count(*) FROM key_package_claim_receipts")
            .await,
        1
    );
    ana.core.stop_sync();
    proxy.disconnect.notify_one();
    ana.reopen();
    ana.core.start_sync(None).unwrap();
    let (retry, retry_targets) = next_request(&mut proxy).await;
    assert_eq!((retry, retry_targets), (operation.clone(), targets));
    let (_, replay, held) = next_reply(&mut proxy).await;
    assert!(!held);
    assert_eq!(
        replay, first,
        "receipt must return the same signed package bytes"
    );
    ana.core.wait_until_settled(8000).await.unwrap();
    assert_eq!(
        bruno.stock(&w).await,
        31,
        "retry must not consume a second package"
    );
    assert_eq!(
        w.count("SELECT count(*) FROM key_package_claim_receipts")
            .await,
        1
    );
    let db = rusqlite::Connection::open(ana.home.join("zoen.sqlite")).unwrap();
    let pending: i64 = db
        .query_row(
            "SELECT count(*) FROM meta WHERE key = ?1",
            [format!("mls.claim:{space}")],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pending, 0, "staged MLS commit atomically retires its claim");
    readable(&w, &ana, &bruno, &space).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn group_cli_deadline_cannot_report_an_unissued_claim_as_complete() {
    let w = World::new("kp_claim_unissued").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let mut proxy = ClaimProxy::new(w.port, Boundary::Request).await;
    let mut ana = Device::new(&w, "ana", &proxy.url);
    ana.start().await;
    ana.core.stop_sync();
    let mut child = Command::new(env!("CARGO_BIN_EXE_zoen"))
        .arg("--home")
        .arg(&ana.home)
        .args(["group", "Unissued", "@bruno"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (operation, targets) = next_request(&mut proxy).await;
    assert_eq!(targets, vec![bruno.account.identity_id.clone()]);
    assert_eq!(
        bruno.stock(&w).await,
        32,
        "the real request has not reached the relay"
    );
    // Default CLI deadline is unchanged; the held transport makes it deterministic.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(9);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("group ignored its default eight-second deadline");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        !output.status.success(),
        "unissued claim falsely completed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("did not finish before the deadline"));
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
    assert_eq!(bruno.stock(&w).await, 32);
    proxy.disconnect.notify_one();
    ana.reopen();
    ana.core.start_sync(None).unwrap();
    let (retry, retry_targets) = next_request(&mut proxy).await;
    assert_eq!((retry, retry_targets), (operation, targets));
    let (_, records, held) = next_reply(&mut proxy).await;
    assert!(!held);
    assert_eq!(records.len(), 1);
    ana.core.wait_until_settled(8000).await.unwrap();
    assert_eq!(bruno.stock(&w).await, 31);
    let space = ana
        .core
        .spaces()
        .into_iter()
        .find(|s| s.title == "Unissued")
        .unwrap()
        .id;
    readable(&w, &ana, &bruno, &space).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_claim_schedules_work_and_cannot_look_idle_before_retry() {
    let w = World::new("kp_claim_retry_idle").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    // An enrolled recipient with nothing published yet. Keep its real agreement key,
    // then let its real core replenish signed packages when it returns online.
    let mut db = PgConnection::connect(&w.db_url).await.unwrap();
    sqlx::query("DELETE FROM key_packages WHERE identity = $1")
        .bind(&bruno.account.identity_id)
        .execute(&mut db)
        .await
        .unwrap();
    let mut proxy = ClaimProxy::new(w.port, Boundary::Reply).await;
    let ana = Device::new(&w, "ana", &proxy.url);
    ana.start().await;
    let space = ana
        .core
        .create_group_with(
            "Scheduled retry".into(),
            vec![bruno.account.identity_id.clone()],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    let (empty_operation, _) = next_request(&mut proxy).await;
    let (_, records, held) = next_reply(&mut proxy).await;
    assert!(held);
    assert!(records.is_empty());
    proxy.release.notify_one();
    assert!(ana.core.wait_until_settled(200).await.is_err());
    let connection = ana.core.connection();
    assert!(connection.synced, "{connection:?}");
    assert_eq!(
        connection.pending, 0,
        "a scheduled claim retry exists outside the event outbox"
    );
    bruno.start().await;
    let (retry, _) = next_request(&mut proxy).await;
    assert_ne!(
        retry, empty_operation,
        "a completed empty claim must not replay emptiness forever"
    );
    let (_, records, held) = next_reply(&mut proxy).await;
    assert!(!held);
    assert_eq!(records.len(), 1);
    ana.core.wait_until_settled(8000).await.unwrap();
    assert_eq!(bruno.stock(&w).await, 31);
    readable(&w, &ana, &bruno, &space).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn protocol_four_without_the_connected_relay_capability_refuses_claims() {
    let w = World::new("kp_claim_capability").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let mut proxy = ClaimProxy::new(w.port, Boundary::AbsentCapability).await;
    let ana = Device::new(&w, "ana", &proxy.url);
    ana.start().await;
    ana.core
        .create_group_with(
            "Needs receipts".into(),
            vec![bruno.account.identity_id.clone()],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    let error = ana.core.wait_until_settled(1000).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not support durable key-package claims"),
        "{error}"
    );
    assert!(
        proxy.events.try_recv().is_err(),
        "unsupported relay received a destructive claim"
    );
    assert_eq!(bruno.stock(&w).await, 32);
    assert_eq!(
        w.count("SELECT count(*) FROM key_package_claim_receipts")
            .await,
        0
    );
}

fn operation_at(at_ms: i64, nonce: u128) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut value = ((at_ms as u128) << 80) | nonce;
    let mut output = [b'0'; 26];
    for ch in output.iter_mut().rev() {
        *ch = ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(output.to_vec()).unwrap()
}

async fn claim(
    client: &mut RawClient,
    targets: Vec<String>,
    operation: &str,
) -> Result<Vec<KeyPackageRecord>, String> {
    match client
        .request(Op::ClaimKeyPackages {
            ids: targets,
            operation_id: Some(operation.into()),
        })
        .await?
    {
        Reply::KeyPackages(records) => Ok(records),
        reply => panic!("unexpected {reply:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn receipts_bind_canonical_targets_and_authenticated_actor() {
    let w = World::new("kp_claim_scope").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let source = Device::new(&w, "ana", &w.relay_url());
    source.start().await;
    source.core.stop_sync();
    let mut ana = RawClient::reconnect(&w.relay_url(), source.author()).await;
    let mut carol = RawClient::connect(&w.relay_url(), "carol").await;
    let operation = operation_at(now_ms(), 1);
    let first = claim(
        &mut ana,
        vec![bruno.account.identity_id.clone(), carol.identity()],
        &operation,
    )
    .await
    .unwrap();
    assert_eq!(first.len(), 1);
    let replay = claim(
        &mut ana,
        vec![
            carol.identity(),
            bruno.account.identity_id.clone(),
            carol.identity(),
        ],
        &operation,
    )
    .await
    .unwrap();
    assert_eq!(
        first, replay,
        "order and duplicate aliases canonicalize to the same targets"
    );
    let error = claim(
        &mut ana,
        vec![bruno.account.identity_id.clone()],
        &operation,
    )
    .await
    .unwrap_err();
    assert!(error.contains("different targets"), "{error}");
    assert_eq!(bruno.stock(&w).await, 31);
    let other = claim(
        &mut carol,
        vec![bruno.account.identity_id.clone()],
        &operation,
    )
    .await
    .unwrap();
    assert_eq!(other.len(), 1);
    assert_ne!(
        other, first,
        "another authenticated identity cannot read Ana's receipt"
    );
    assert_eq!(bruno.stock(&w).await, 30);
    let mut again = RawClient::reconnect(&w.relay_url(), ana.author.clone()).await;
    assert_eq!(
        claim(
            &mut again,
            vec![carol.identity(), bruno.account.identity_id.clone()],
            &operation
        )
        .await
        .unwrap(),
        first
    );
    assert_eq!(bruno.stock(&w).await, 30);
    let root: [u8; 32] = source
        .vault
        .load("zoen.identity.v1".into())
        .unwrap()
        .try_into()
        .unwrap();
    let second = Author::device(&Signer::from_secret(&root), Signer::generate());
    ana.request(Op::DeliverLink {
        id: Signer::generate().id(),
        sealed: vec![1],
        device: Some(roda_proto::DeviceCertificate {
            device: second.device.clone().unwrap(),
            cert: second.cert.clone().unwrap(),
        }),
    })
    .await
    .unwrap();
    let mut other_device = RawClient::reconnect(&w.relay_url(), second).await;
    let independent = claim(
        &mut other_device,
        vec![bruno.account.identity_id.clone()],
        &operation,
    )
    .await
    .unwrap();
    assert_ne!(
        independent, first,
        "a different enrolled device has a separate operation scope"
    );
    assert_eq!(bruno.stock(&w).await, 29);
    // Revocation still fences outgoing receipt delivery and future admission.
    let device = ana.author.device.clone().unwrap();
    other_device.request(Op::Unlink { device }).await.unwrap();
    while ana.recv_or_close().await.is_some() {}
    let _ = again
        .send_if_open(&roda_proto::ClientFrame::Req {
            id: 99,
            op: Op::ClaimKeyPackages {
                ids: vec![bruno.account.identity_id.clone()],
                operation_id: Some(operation),
            },
        })
        .await;
    while let Some(frame) = again.recv_or_close().await {
        assert!(!matches!(
            frame,
            ServerFrame::Res {
                id: 99,
                result: Ok(Reply::KeyPackages(_))
            }
        ));
    }
    assert_eq!(bruno.stock(&w).await, 29);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_future_and_noncanonical_operations_never_consume_packages() {
    let w = World::new("kp_claim_expiry").await;
    let bruno = Device::new(&w, "bruno", &w.relay_url());
    bruno.start().await;
    bruno.core.stop_sync();
    let mut ana = RawClient::connect(&w.relay_url(), "ana").await;
    let now = now_ms();
    for operation in [
        operation_at(now - KEY_PACKAGE_CLAIM_TTL_MS - 1000, 1),
        operation_at(now + 60_000, 2),
        "Z0000000000000000000000000".into(),
        operation_at(now, 3).to_lowercase(),
        "prefix-00000000000000000000000000".into(),
    ] {
        assert!(claim(
            &mut ana,
            vec![bruno.account.identity_id.clone()],
            &operation
        )
        .await
        .is_err());
    }
    assert_eq!(bruno.stock(&w).await, 32);
    let expired = operation_at(now - KEY_PACKAGE_CLAIM_TTL_MS - 1000, 7);
    let mut db = PgConnection::connect(&w.db_url).await.unwrap();
    sqlx::query("INSERT INTO key_package_claim_receipts(source_identity,source_device,operation_id,targets,response,expires_at_ms) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(ana.identity()).bind(ana.author.device.clone().unwrap()).bind(&expired)
        .bind(vec![bruno.account.identity_id.clone()]).bind(ServerFrame::Res { id: 0, result: Ok(Reply::KeyPackages(Vec::new())) }.encode()).bind(now - 1000)
        .execute(&mut db).await.unwrap();
    // Same deletion performed by actor cleanup / indexed background GC. Absence of an
    // old receipt must not reinterpret an expired operation as a fresh destructive claim.
    sqlx::query("DELETE FROM key_package_claim_receipts WHERE expires_at_ms < $1")
        .bind(now)
        .execute(&mut db)
        .await
        .unwrap();
    assert_eq!(
        claim(&mut ana, vec![bruno.account.identity_id.clone()], &expired)
            .await
            .unwrap_err(),
        KEY_PACKAGE_CLAIM_EXPIRED
    );
    assert_eq!(bruno.stock(&w).await, 32);
}
