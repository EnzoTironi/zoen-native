//! Linking a second device, from the apps and the CLI (ADR 0045).
//!
//! New device: [`Client::link_request`] (shows the code), [`Client::link_wait`]
//! (becomes the account), then [`Client::receive_history`] (the first bundle) and
//! [`Client::load_older`] when the person scrolls up. Existing device:
//! [`Client::link_device`], then [`Client::send_history`] once the new device is in
//! every group.

use std::{
    collections::BTreeSet,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use base64::Engine as _;
use event_log::Signer;
use protocol::{transfer_message, DeviceCertificate, Op, Reply};

use crate::{
    api::SecretVault,
    i18n::t,
    link::{self, HistoryBox, IdentityBox, LinkCode, Peer, Pending},
    linking::{META_INITIAL_HISTORY_PENDING, META_PRIMARY},
    net::http_base,
    profile::VAULT_AGREEMENT,
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    AccountDto, Client, CoreError,
};

const VAULT_PEERS: &str = "link.peers";
const META_HISTORY: &str = "link.history";
const META_HISTORY_COMPLETE: &str = "link.initial_history_complete";
const MAX_HISTORY_BYTES: usize = 64 << 20;
const MAX_HISTORY_CHUNK: usize = (1 << 20) + 16;
const MAX_HISTORY_EVENTS: usize = 100_000;

/// UniFFI's Swift future cancellation isn't sufficient to stop native work. This
/// control cancels awaits explicitly and refuses overlapping receivers per engine.
pub(crate) struct HistoryControl {
    active: Arc<AtomicBool>,
    cancellation: tokio::sync::watch::Sender<u64>,
}
impl Default for HistoryControl {
    fn default() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            cancellation: tokio::sync::watch::channel(0).0,
        }
    }
}
impl HistoryControl {
    fn begin(&self, timeout_ms: u64) -> Result<HistoryReceive, CoreError> {
        if !(1..=120_000).contains(&timeout_ms) {
            return Err(invalid(
                "History waits must be between 1 and 120000 milliseconds.",
            ));
        }
        let cancellation = self.cancellation.subscribe();
        let epoch = *cancellation.borrow();
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(invalid(t(
                "O histórico já está sendo recebido.",
                "History is already being received.",
            )));
        }
        Ok(HistoryReceive {
            active: self.active.clone(),
            cancellation,
            epoch,
            deadline: tokio::time::Instant::now() + Duration::from_millis(timeout_ms),
        })
    }
    fn cancel(&self) {
        self.cancellation
            .send_modify(|epoch| *epoch = epoch.wrapping_add(1));
    }
}
struct HistoryReceive {
    active: Arc<AtomicBool>,
    cancellation: tokio::sync::watch::Receiver<u64>,
    epoch: u64,
    deadline: tokio::time::Instant,
}
impl HistoryReceive {
    fn check(&self) -> Result<(), CoreError> {
        if *self.cancellation.borrow() != self.epoch {
            return Err(invalid("History receive cancelled."));
        }
        if tokio::time::Instant::now() >= self.deadline {
            return Err(invalid(t(
                "O histórico ainda não chegou. A transferência continuará depois.",
                "History has not arrived yet. The transfer will continue later.",
            )));
        }
        Ok(())
    }
    async fn wait<T>(
        &mut self,
        work: impl Future<Output = Result<T, CoreError>>,
    ) -> Result<T, CoreError> {
        self.check()?;
        let result = tokio::select! {
            _ = self.cancellation.changed() => Err(invalid("History receive cancelled.")),
            _ = tokio::time::sleep_until(self.deadline) => Err(invalid(t("O histórico ainda não chegou. A transferência continuará depois.", "History has not arrived yet. The transfer will continue later."))),
            value = work => value,
        };
        self.check()?;
        result
    }
}
impl Drop for HistoryReceive {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

fn valid_history_manifest(manifest: &HistoryBox) -> bool {
    let hash = |value: &str| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit());
    hash(&manifest.transfer)
        && hash(&manifest.key)
        && !manifest.chunks.is_empty()
        && manifest.chunks.len() <= 900
        && manifest.chunks.iter().all(|value| hash(value))
}

fn chunk_meta(n: usize) -> String {
    format!("link.chunk:{n}")
}

/// Bytes per history chunk (`ZOEN_TRANSFER_CHUNK`; journeys use a small one).
fn chunk_size() -> usize {
    std::env::var("ZOEN_TRANSFER_CHUNK")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n >= 256)
        .unwrap_or(1024 * 1024)
}

fn invalid(reason: impl Into<String>) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}

/// Optional authenticated provider account, pinned before any recovery side effect.
pub(crate) struct ExpectedIdentity(Option<String>);
impl ExpectedIdentity {
    pub(crate) fn parse(value: Option<String>) -> Result<Self, CoreError> {
        if value
            .as_ref()
            .is_some_and(|identity| event_log::parse_device_key(identity).is_none())
        {
            return Err(invalid(t(
                "A conta esperada é inválida.",
                "The expected account is invalid.",
            )));
        }
        Ok(Self(value.map(|identity| identity.to_ascii_lowercase())))
    }
    pub(crate) fn check(&self, actual: &str) -> Result<(), CoreError> {
        if self.0.as_ref().is_some_and(|expected| expected != actual) {
            return Err(invalid(t(
                "Este vínculo ou backup pertence a outra conta. Use a conta confirmada no login.",
                "This link or backup belongs to another account. Use the account confirmed during sign in.",
            )));
        }
        Ok(())
    }
}

/// Progress of a history transfer, chunk by chunk.
#[uniffi::export(with_foreign)]
pub trait TransferListener: Send + Sync {
    fn on_progress(&self, done: u32, total: u32);
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct LinkRequestDto {
    /// What the QR code carries.
    pub code: String,
    /// Six digits both screens show.
    pub check: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct LinkedDto {
    pub device_id: String,
    pub check: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct HistoryDto {
    pub messages: u32,
    pub chunks: u32,
    /// Chunks already here from an earlier, interrupted try.
    pub resumed: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct HistoryStatusDto {
    pub initial_pending: bool,
    pub manifest_received: bool,
    pub chunks_received: u32,
    pub chunks_total: u32,
    pub receive_active: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct OlderDto {
    pub loaded: u32,
    /// Older messages are left on the other device.
    pub more: bool,
    /// The device that has them didn't answer: it's offline.
    pub primary_offline: bool,
    /// What to show when it is offline.
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct DeviceDto {
    pub device_id: String,
    pub revoked: bool,
    pub this_device: bool,
}

fn http() -> Result<reqwest::Client, CoreError> {
    crate::https::client()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| invalid(e.to_string()))
}

fn save_peers(vault: &Arc<dyn SecretVault>, peers: &[Peer]) -> bool {
    vault.save(
        VAULT_PEERS.into(),
        serde_json::to_vec(peers).unwrap_or_default(),
    )
}

pub(crate) fn load_peers(vault: &Arc<dyn SecretVault>) -> Vec<Peer> {
    vault
        .load(VAULT_PEERS.into())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn secret32(b: Option<Vec<u8>>) -> Option<[u8; 32]> {
    b.and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
}

#[uniffi::export]
impl Client {
    /// Status is derived from the canonical device database; no UI-local completion flag.
    pub fn history_status(&self) -> Result<HistoryStatusDto, CoreError> {
        self.history_status_impl()
    }

    pub fn cancel_history_receive(&self) {
        self.cancel_history_receive_impl()
    }

    /// Permanently fences provisional linking on this Client before owner teardown.
    /// It revokes key use and installation synchronously; it does not wait for task Drop.
    pub fn retire_link_pickup(&self) {
        self.retire_link_pickup_impl()
    }
}

#[uniffi::export]
impl Client {
    /// New device: makes the code to show (as a QR) and keeps what it needs to finish.
    pub fn link_request(
        &self,
        relay_url: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<LinkRequestDto, CoreError> {
        self.link_request_impl(relay_url, vault)
    }

    /// Existing device: devices of this account, as the relay lists them.
    pub fn link_code_check(&self, code: String) -> Result<String, CoreError> {
        self.link_code_check_impl(code)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Client {
    /// Waits for actual provisional worker and caller retirement after the owner
    /// has called retire_link_pickup. No successful timeout or detached cleanup.
    pub async fn drain_link_pickup(&self) {
        self.drain_link_pickup_impl().await
    }

    /// New device: waits for the existing device's box, checks it and becomes the account.
    /// Start sync afterwards.
    pub async fn link_wait(
        &self,
        vault: Arc<dyn SecretVault>,
        timeout_ms: u64,
    ) -> Result<AccountDto, CoreError> {
        self.link_wait_impl(vault, timeout_ms).await
    }

    /// Existing device: hands the account to the device showing `code`, and asks for it
    /// in every group. Then [`Self::send_history`] once it is in them.
    pub async fn link_device(
        &self,
        code: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<LinkedDto, CoreError> {
        self.link_device_impl(code, vault).await
    }

    /// Existing device: whether `device` is in every group yet, as (in, of).
    pub async fn link_progress(&self, device_id: String) -> Vec<u32> {
        self.link_progress_impl(device_id).await
    }

    /// Existing device: seals the recent window of every chat for `device_id`, puts it on
    /// the relay chunk by chunk and leaves the key for it. Putting again resumes.
    pub async fn send_history(
        &self,
        device_id: String,
        listener: Option<Arc<dyn TransferListener>>,
    ) -> Result<u32, CoreError> {
        self.send_history_impl(device_id, listener).await
    }

    /// New device: takes the first history bundle (waiting for it up to `timeout_ms`),
    /// downloads what isn't here yet, opens it into the chats and deletes it from the relay.
    pub async fn receive_history(
        &self,
        timeout_ms: u64,
        listener: Option<Arc<dyn TransferListener>>,
    ) -> Result<HistoryDto, CoreError> {
        self.receive_history_impl(timeout_ms, listener).await
    }

    /// New device: asks the device that linked it for an older page of `space_id`.
    pub async fn load_older(
        &self,
        space_id: String,
        timeout_ms: u64,
    ) -> Result<OlderDto, CoreError> {
        self.load_older_impl(space_id, timeout_ms).await
    }

    /// Unlinks one of this account's devices: the relay stops letting it in, and this
    /// device takes it out of every group.
    pub async fn unlink_device(&self, device_id: String) -> Result<(), CoreError> {
        self.unlink_device_impl(device_id).await
    }

    pub async fn devices(&self) -> Result<Vec<DeviceDto>, CoreError> {
        self.devices_impl().await
    }
}

#[cfg(test)]
mod pin_tests;

#[cfg(test)]
mod initial_history_tests;

mod directory;
mod enroll_prepare;
mod enrollment;
#[cfg(test)]
mod enrollment_owner_tests;
mod history_commit;
#[cfg(test)]
mod history_owner_tests;
mod history_state;
mod owner;
#[cfg(test)]
mod test_transport;
mod transfer;
