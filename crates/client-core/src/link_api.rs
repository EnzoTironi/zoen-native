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
        let _language = crate::i18n::enter(self.lang);
        let e = self.lock();
        let manifest = e
            .store
            .meta(META_HISTORY)?
            .map(|json| {
                serde_json::from_str::<HistoryBox>(&json)
                    .map_err(|_| invalid("Invalid retained history manifest."))
            })
            .transpose()?;
        if manifest
            .as_ref()
            .is_some_and(|value| !valid_history_manifest(value))
        {
            return Err(invalid("Invalid retained history manifest."));
        }
        let mut received = 0;
        if let Some(manifest) = &manifest {
            for n in 0..manifest.chunks.len() {
                if e.store.meta(&chunk_meta(n))?.is_some() {
                    received += 1;
                }
            }
        }
        Ok(HistoryStatusDto {
            initial_pending: e.store.meta(META_INITIAL_HISTORY_PENDING)?.is_some()
                || manifest.is_some(),
            manifest_received: manifest.is_some(),
            chunks_received: received,
            chunks_total: manifest.map_or(0, |value| value.chunks.len() as u32),
            receive_active: self.history_receive.active.load(Ordering::Acquire),
        })
    }

    pub fn cancel_history_receive(&self) {
        let _language = crate::i18n::enter(self.lang);
        self.history_receive.cancel();
    }

    /// Permanently fences provisional linking on this Client before owner teardown.
    /// It revokes key use and installation synchronously; it does not wait for task Drop.
    pub fn retire_link_pickup(&self) {
        self.link_pickup.retire();
    }
}

impl Client {
    fn signed_headers(
        &self,
        msg: impl Fn(i64) -> Vec<u8>,
    ) -> Result<(String, String, String), CoreError> {
        let e = self.lock();
        let author = e
            .net
            .author
            .as_ref()
            .ok_or_else(|| invalid(t("Entre na sua conta primeiro.", "Sign in first.")))?;
        let ts = crate::engine::now_ms();
        Ok((author.key.id(), ts.to_string(), author.key.sign(&msg(ts))))
    }

    fn relay_base(&self) -> Result<String, CoreError> {
        let e = self.lock();
        let a = e.account().ok_or_else(|| invalid("account"))?;
        Ok(http_base(&a.relay_url))
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
        self.link_pickup.with_request(|| {
            if self.lock().account().is_some() {
                return Err(invalid(t(
                    "Este aparelho já tem uma conta.",
                    "This device already has an account.",
                )));
            }
            let device = Signer::generate();
            let (hpke_secret, hpke_public) = link::key_pair();
            let secret = link::random::<32>();
            let code = LinkCode {
                device: device.id(),
                hpke: hpke_public.clone(),
                secret,
            };
            let pending = Pending {
                device_secret: hex::encode(device.secret()),
                hpke_secret: hex::encode(hpke_secret),
                hpke_public: hex::encode(hpke_public),
                secret: hex::encode(secret),
                relay: relay_url,
            };
            if !vault.save(
                link::VAULT_PENDING.into(),
                serde_json::to_vec(&pending).unwrap_or_default(),
            ) {
                return Err(invalid(t(
                    "Não deu para guardar a chave no Keychain.",
                    "Couldn't store the key in the Keychain.",
                )));
            }
            Ok(LinkRequestDto {
                code: code.encode(),
                check: code.check_digits(),
            })
        })
    }

    /// Existing device: devices of this account, as the relay lists them.
    pub fn link_code_check(&self, code: String) -> Result<String, CoreError> {
        LinkCode::parse(&code)
            .map(|c| c.check_digits())
            .ok_or_else(|| invalid(t("Código inválido.", "Invalid code.")))
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Client {
    /// Waits for actual provisional worker and caller retirement after the owner
    /// has called retire_link_pickup. No successful timeout or detached cleanup.
    pub async fn drain_link_pickup(&self) {
        self.link_pickup.drain().await;
    }

    /// New device: waits for the existing device's box, checks it and becomes the account.
    /// Start sync afterwards.
    pub async fn link_wait(
        &self,
        vault: Arc<dyn SecretVault>,
        timeout_ms: u64,
    ) -> Result<AccountDto, CoreError> {
        self.link_wait_expected(vault, timeout_ms, None).await
    }

    /// Existing device: hands the account to the device showing `code`, and asks for it
    /// in every group. Then [`Self::send_history`] once it is in them.
    pub async fn link_device(
        &self,
        code: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<LinkedDto, CoreError> {
        let code = LinkCode::parse(&code)
            .ok_or_else(|| invalid(t("Código inválido.", "Invalid code.")))?;
        let identity = vault.load(VAULT_IDENTITY.into()).ok_or_else(|| {
            invalid(t(
                "A chave da conta não está aqui.",
                "The account key isn't here.",
            ))
        })?;
        let identity =
            Signer::from_secret(&secret32(Some(identity)).ok_or_else(|| invalid("identity"))?);
        let agreement = vault.load(VAULT_AGREEMENT.into()).unwrap_or_default();
        let (my_hpke_secret, my_hpke_public) = link::key_pair();
        let b = {
            let e = self.lock();
            let a = e.account().ok_or_else(|| invalid("account"))?.clone();
            if identity.id() != a.identity {
                return Err(invalid("identity"));
            }
            let me = e.my_profile().ok_or_else(|| invalid("account"))?;
            let (profile_key, profile_key_version, profile_seen_version, profile_fields) =
                e.profile_for_link()?;
            IdentityBox {
                identity_secret: hex::encode(identity.secret()),
                agreement_secret: hex::encode(&agreement),
                cert: identity.sign(&event_log::device_cert_message(&code.device)),
                name: me.name,
                handle: me.handle,
                relay: a.relay_url.clone(),
                primary: a.device.clone(),
                primary_hpke: hex::encode(&my_hpke_public),
                profile_key,
                profile_key_version,
                profile_seen_version,
                profile_fields,
            }
        };
        let plain = serde_json::to_vec(&b).map_err(|e| invalid(e.to_string()))?;
        let sealed = link::seal(&code.hpke, &code.secret, "identity", &plain)
            .ok_or_else(|| invalid("sealing"))?;
        let peer = Peer {
            device: code.device.clone(),
            their_hpke: hex::encode(&code.hpke),
            my_hpke_secret: hex::encode(&my_hpke_secret),
            secret: hex::encode(code.secret),
        };
        let mut peers = load_peers(&vault);
        peers.retain(|p| p.device != peer.device);
        peers.push(peer.clone());
        if !save_peers(&vault, &peers) {
            return Err(invalid(t(
                "Não deu para guardar a chave no Keychain.",
                "Couldn't store the key in the Keychain.",
            )));
        }
        self.lock().add_peer(peer);
        match self
            .request(Op::DeliverLink {
                id: link::box_id(&code.secret, "identity"),
                sealed,
                device: Some(DeviceCertificate {
                    device: code.device.clone(),
                    cert: b.cert,
                }),
            })
            .await?
        {
            Reply::Done => {}
            _ => return Err(invalid("the relay answered something else")),
        }
        self.lock().device_linked(&code.device)?;
        Ok(LinkedDto {
            check: code.check_digits(),
            device_id: code.device,
        })
    }

    /// Existing device: whether `device` is in every group yet, as (in, of).
    pub async fn link_progress(&self, device_id: String) -> Vec<u32> {
        let (done, total) = self.lock().link_progress(&device_id);
        vec![done, total]
    }

    /// Existing device: seals the recent window of every chat for `device_id`, puts it on
    /// the relay chunk by chunk and leaves the key for it. Putting again resumes.
    pub async fn send_history(
        &self,
        device_id: String,
        listener: Option<Arc<dyn TransferListener>>,
    ) -> Result<u32, CoreError> {
        let (bundle, peer) = {
            let e = self.lock();
            let peer = e.net.link.peers.get(&device_id).cloned().ok_or_else(|| {
                invalid(t(
                    "Aparelho não vinculado aqui.",
                    "Not a device linked here.",
                ))
            })?;
            (e.history_bundle(), peer)
        };
        let plain = serde_json::to_vec(&bundle).map_err(|e| invalid(e.to_string()))?;
        let key = link::random::<32>();
        let transfer = hex::encode(link::random::<32>());
        let base = self.relay_base()?;
        let client = http()?;
        let pieces: Vec<&[u8]> = plain.chunks(chunk_size()).collect();
        let total = pieces.len() as u32;
        let mut hashes = Vec::new();
        for (n, piece) in pieces.iter().enumerate() {
            let sealed = link::seal_chunk(&key, n as u32, piece);
            let sha = event_log::sha256_hex(&sealed);
            let (device, ts, sig) = self.signed_headers(|ts| {
                transfer_message("put", &transfer, &n.to_string(), &sha, ts)
            })?;
            let r = client
                .put(format!("{base}/v1/transfer/{transfer}/{n}"))
                .header("x-zoen-device", device)
                .header("x-zoen-ts", ts)
                .header("x-zoen-sig", sig)
                .body(sealed)
                .send()
                .await
                .map_err(|e| invalid(e.to_string()))?;
            if !r.status().is_success() {
                return Err(invalid(format!("history upload: {}", r.status())));
            }
            hashes.push(sha);
            if let Some(l) = &listener {
                l.on_progress(n as u32 + 1, total);
            }
        }
        let manifest = HistoryBox {
            transfer,
            key: hex::encode(key),
            chunks: hashes,
        };
        let secret = peer.secret().ok_or_else(|| invalid("peer"))?;
        let to = hex::decode(&peer.their_hpke).unwrap_or_default();
        let sealed = link::seal(
            &to,
            &secret,
            "history",
            &serde_json::to_vec(&manifest).unwrap_or_default(),
        )
        .ok_or_else(|| invalid("sealing"))?;
        match self
            .request(Op::DeliverLink {
                id: link::box_id(&secret, "history"),
                sealed,
                device: None,
            })
            .await?
        {
            Reply::Done => {
                // A best-effort wake only after the manifest exists durably. The
                // destination's pending marker + foreground/reconnect resume are fallback.
                self.lock().announce_initial_history(&device_id);
                Ok(bundle.events.len() as u32)
            }
            _ => Err(invalid("the relay answered something else")),
        }
    }

    /// New device: takes the first history bundle (waiting for it up to `timeout_ms`),
    /// downloads what isn't here yet, opens it into the chats and deletes it from the relay.
    pub async fn receive_history(
        &self,
        timeout_ms: u64,
        listener: Option<Arc<dyn TransferListener>>,
    ) -> Result<HistoryDto, CoreError> {
        crate::i18n::scope(self.lang, async {
            let mut receive = self.history_receive.begin(timeout_ms)?;
            {
                let e = self.lock();
                if e.store.meta(META_INITIAL_HISTORY_PENDING)?.is_none() {
                    if let Some(receipt) = e.store.meta(META_HISTORY_COMPLETE)? {
                        return serde_json::from_str(&receipt)
                            .map_err(|_| invalid("Invalid history receipt."));
                    }
                }
                if e.store.meta(META_PRIMARY)?.is_none() {
                    return Err(invalid("This device was not linked."));
                }
                // Legacy manual retries also become resumable before fetching the one-time box.
                e.store.set_meta(META_INITIAL_HISTORY_PENDING, "1")?;
            }
            let manifest = self.history_manifest(&mut receive).await?;
            let key =
                secret32(hex::decode(&manifest.key).ok()).ok_or_else(|| invalid("history"))?;
            let base = self.relay_base()?;
            let client = http()?;
            let total = manifest.chunks.len();
            let stop_after: Option<usize> = std::env::var("ZOEN_TRANSFER_STOP_AFTER")
                .ok()
                .and_then(|v| v.parse().ok());
            let mut resumed = 0;
            let mut fetched = 0;
            let b64 = base64::engine::general_purpose::STANDARD;
            for (n, sha) in manifest.chunks.iter().enumerate() {
                receive.check()?;
                if let Some(retained) = self.lock().store.meta(&chunk_meta(n))? {
                    if b64.decode(retained).ok().is_some_and(|bytes| {
                        bytes.len() <= MAX_HISTORY_CHUNK && event_log::sha256_hex(&bytes) == *sha
                    }) {
                        resumed += 1;
                        continue;
                    }
                    return Err(invalid("A retained history chunk does not match."));
                }
                if stop_after.is_some_and(|s| fetched >= s) {
                    return Err(invalid(format!("history stopped at {}/{total}", n)));
                }
                let mut r = receive
                    .wait(async {
                        client
                            .get(format!("{base}/v1/transfer/{}/{n}", manifest.transfer))
                            .send()
                            .await
                            .map_err(|e| invalid(e.to_string()))
                    })
                    .await?;
                if !r.status().is_success() {
                    return Err(invalid(format!("history download: {}", r.status())));
                }
                if r.content_length()
                    .is_some_and(|length| length > MAX_HISTORY_CHUNK as u64)
                {
                    return Err(invalid("A history chunk is too large."));
                }
                let mut bytes = Vec::new();
                while let Some(part) = receive
                    .wait(async { r.chunk().await.map_err(|e| invalid(e.to_string())) })
                    .await?
                {
                    if bytes.len().saturating_add(part.len()) > MAX_HISTORY_CHUNK {
                        return Err(invalid("A history chunk is too large."));
                    }
                    bytes.extend_from_slice(&part);
                }
                if event_log::sha256_hex(&bytes) != *sha {
                    return Err(invalid("a history chunk doesn't match"));
                }
                receive.check()?;
                self.lock()
                    .store
                    .set_meta(&chunk_meta(n), &b64.encode(&bytes))?;
                fetched += 1;
                if let Some(l) = &listener {
                    l.on_progress((n + 1) as u32, total as u32);
                }
            }
            let mut plain = Vec::new();
            for n in 0..total {
                receive.check()?;
                let sealed = self
                    .lock()
                    .store
                    .meta(&chunk_meta(n))?
                    .and_then(|s| b64.decode(s).ok())
                    .ok_or_else(|| invalid("history chunk"))?;
                if sealed.len() > MAX_HISTORY_CHUNK
                    || event_log::sha256_hex(&sealed) != manifest.chunks[n]
                {
                    return Err(invalid("A retained history chunk does not match."));
                }
                let opened = link::open_chunk(&key, n as u32, &sealed)
                    .ok_or_else(|| invalid("A history chunk does not open."))?;
                if plain.len().saturating_add(opened.len()) > MAX_HISTORY_BYTES {
                    return Err(invalid("The history bundle is too large."));
                }
                plain.extend(opened);
            }
            let bundle: link::Bundle =
                serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
            if bundle.events.len() > MAX_HISTORY_EVENTS {
                return Err(invalid("The history bundle has too many events."));
            }
            // Receiving the box does not mean the authenticated relay chain is already here.
            while !self.connection().synced {
                receive
                    .wait(async {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        Ok(())
                    })
                    .await?;
            }
            let receipt =
                self.commit_initial_history(bundle.events, total as u32, resumed, &receive)?;
            // Local completion was committed before this best-effort relay cleanup.
            let (device, ts, sig) = self
                .signed_headers(|ts| transfer_message("delete", &manifest.transfer, "", "", ts))?;
            let _ = receive
                .wait(async {
                    client
                        .delete(format!("{base}/v1/transfer/{}", manifest.transfer))
                        .header("x-zoen-device", device)
                        .header("x-zoen-ts", ts)
                        .header("x-zoen-sig", sig)
                        .send()
                        .await
                        .map_err(|e| invalid(e.to_string()))
                })
                .await;
            Ok(receipt)
        })
        .await
    }

    /// New device: asks the device that linked it for an older page of `space_id`.
    pub async fn load_older(
        &self,
        space_id: String,
        timeout_ms: u64,
    ) -> Result<OlderDto, CoreError> {
        let Some(req) = self.lock().request_page(&space_id) else {
            return Ok(OlderDto {
                loaded: 0,
                more: false,
                primary_offline: false,
                message: None,
            });
        };
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if let Some((loaded, more)) = self.lock().page_answer(&req) {
                return Ok(OlderDto {
                    loaded,
                    more,
                    primary_offline: false,
                    message: None,
                });
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(OlderDto {
                    loaded: 0,
                    more: true,
                    primary_offline: true,
                    message: Some(t(
                        "Abra o Zoen no celular para carregar mensagens antigas.",
                        "Open Zoen on your phone to load older messages.",
                    )),
                });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Unlinks one of this account's devices: the relay stops letting it in, and this
    /// device takes it out of every group.
    pub async fn unlink_device(&self, device_id: String) -> Result<(), CoreError> {
        match self
            .request(Op::Unlink {
                device: device_id.clone(),
            })
            .await?
        {
            Reply::Done => self.lock().unlinked(&device_id),
            _ => Err(invalid("the relay answered something else")),
        }
    }

    pub async fn devices(&self) -> Result<Vec<DeviceDto>, CoreError> {
        let Reply::Devices(list) = self.request(Op::Devices).await? else {
            return Err(invalid("the relay answered something else"));
        };
        let mine = self
            .lock()
            .account()
            .map(|a| a.device.clone())
            .unwrap_or_default();
        self.lock().devices_arrived(list.clone());
        Ok(list
            .into_iter()
            .map(|d| DeviceDto {
                this_device: d.device == mine,
                device_id: d.device,
                revoked: d.revoked,
            })
            .collect())
    }
}

impl Client {
    /// The history box: from the device database if an earlier try got it, else from the
    /// relay (one-time), kept until the bundle is in.
    async fn history_manifest(
        &self,
        receive: &mut HistoryReceive,
    ) -> Result<HistoryBox, CoreError> {
        if let Some(json) = self.lock().store.meta(META_HISTORY)? {
            let m: HistoryBox = serde_json::from_str(&json)
                .map_err(|_| invalid("Invalid retained history manifest."))?;
            if !valid_history_manifest(&m) {
                return Err(invalid("Invalid retained history manifest."));
            }
            return Ok(m);
        }
        let peer = {
            let e = self.lock();
            let primary = e.store.meta(META_PRIMARY)?.ok_or_else(|| {
                invalid(t(
                    "Este aparelho não foi vinculado.",
                    "This device wasn't linked.",
                ))
            })?;
            e.net
                .link
                .peers
                .get(&primary)
                .cloned()
                .ok_or_else(|| invalid("peer"))?
        };
        let secret = peer.secret().ok_or_else(|| invalid("peer"))?;
        let id = link::box_id(&secret, "history");
        let sealed = loop {
            match receive
                .wait(self.request(Op::FetchLink { id: id.clone() }))
                .await?
            {
                Reply::Link(Some(b)) => break b,
                Reply::Link(None) => {}
                _ => return Err(invalid("the relay answered something else")),
            }
            receive
                .wait(async {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                    Ok(())
                })
                .await?;
        };
        if sealed.len() > 65_536 {
            return Err(invalid("The history manifest is too large."));
        }
        let sk = hex::decode(&peer.my_hpke_secret).unwrap_or_default();
        let plain = link::open(&sk, &secret, "history", &sealed)
            .ok_or_else(|| invalid(t("O histórico não abriu.", "The history didn't open.")))?;
        let m: HistoryBox = serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
        if !valid_history_manifest(&m) {
            return Err(invalid("Invalid history manifest."));
        }
        receive.check()?;
        self.lock()
            .store
            .set_meta(META_HISTORY, &serde_json::to_string(&m).unwrap_or_default())?;
        Ok(m)
    }

    /// Import and completion marker share one SQLite commit. A missing relay record
    /// is pending work, never an empty successful history transfer.
    fn commit_initial_history(
        &self,
        events: Vec<types::Event>,
        chunks: u32,
        resumed: u32,
        receive: &HistoryReceive,
    ) -> Result<HistoryDto, CoreError> {
        receive.check()?;
        let mut e = self.lock();
        let mut positions = BTreeSet::new();
        let mut opened = Vec::new();
        for event in events {
            receive.check()?;
            if !positions.insert((event.space.clone(), event.seq)) {
                return Err(invalid("The history repeats a relay position."));
            }
            let local = e
                .logs
                .get(&event.space)
                .and_then(|log| {
                    usize::try_from(event.seq)
                        .ok()
                        .and_then(|index| log.events().get(index))
                })
                .ok_or_else(|| {
                    invalid(t(
                        "O histórico aguarda os registros verificados do servidor.",
                        "History is waiting for the verified relay records.",
                    ))
                })?;
            if local.sealed_wire.is_some() {
                // A retry after a former partial import must be the exact same opened event.
                let mut anchored = event;
                anchored.prev = local.prev.clone();
                anchored.hash = local.hash.clone();
                if &anchored != local {
                    return Err(invalid("An opened history entry does not match."));
                }
                continue;
            }
            let event = link::opened_history_event(local, event).ok_or_else(|| {
                invalid("A history entry does not match the signed relay record.")
            })?;
            opened.push(event);
        }
        let receipt = HistoryDto {
            messages: opened.len() as u32,
            chunks,
            resumed,
        };
        {
            let tx = e
                .store
                .conn()
                .unchecked_transaction()
                .map_err(|err| CoreError::Storage {
                    message: err.to_string(),
                })?;
            for event in opened {
                receive.check()?;
                if !e.store.replace_event(&event)? {
                    return Err(invalid("A history relay record disappeared."));
                }
            }
            e.store.set_meta(
                META_HISTORY_COMPLETE,
                &serde_json::to_string(&receipt).map_err(|err| invalid(err.to_string()))?,
            )?;
            for n in 0..chunks {
                e.store.meta_delete(&chunk_meta(n as usize))?;
            }
            e.store.meta_delete(META_HISTORY)?;
            e.store.meta_delete(META_INITIAL_HISTORY_PENDING)?;
            receive.check()?;
            tx.commit().map_err(|err| CoreError::Storage {
                message: err.to_string(),
            })?;
        }
        e.reload()?;
        Ok(receipt)
    }
}

impl Client {
    pub(crate) async fn link_wait_expected(
        &self,
        vault: Arc<dyn SecretVault>,
        timeout_ms: u64,
        expected_identity_id: Option<String>,
    ) -> Result<AccountDto, CoreError> {
        let expected = ExpectedIdentity::parse(expected_identity_id)?;
        let mut pickup = self.link_pickup.begin(
            || {
                let bytes = vault
                    .load(link::VAULT_PENDING.into())
                    .map(zeroize::Zeroizing::new)
                    .ok_or_else(|| invalid(t("Nenhum pedido de vínculo.", "No link request.")))?;
                serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid link request."))
            },
            Duration::from_millis(timeout_ms),
            self.lang,
        )?;
        let sealed = pickup.wait().await?;
        pickup.with_install(|pending, device| {
            let secret = zeroize::Zeroizing::new(
                secret32(hex::decode(&pending.secret).ok())
                    .ok_or_else(|| invalid("link request"))?,
            );
            let hpke_secret =
                zeroize::Zeroizing::new(hex::decode(&pending.hpke_secret).unwrap_or_default());
            let plain = zeroize::Zeroizing::new(
                link::open(&hpke_secret, &secret, "identity", &sealed)
                    .ok_or_else(|| invalid(t("O vínculo não abriu.", "The link didn't open.")))?,
            );
            let b: IdentityBox =
                serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
            self.finish_link(pending, device, b, vault, &expected)
        })
    }

    fn finish_link(
        &self,
        pending: &Pending,
        device: &Signer,
        b: IdentityBox,
        vault: Arc<dyn SecretVault>,
        expected: &ExpectedIdentity,
    ) -> Result<AccountDto, CoreError> {
        let identity = Signer::from_secret(
            &secret32(hex::decode(&b.identity_secret).ok()).ok_or_else(|| invalid("identity"))?,
        );
        expected.check(&identity.id())?;
        // The certificate must be the account's, for this device.
        if !event_log::verify_sig(
            &identity.id(),
            &event_log::device_cert_message(&device.id()),
            &b.cert,
        ) {
            return Err(invalid(t(
                "O certificado deste aparelho não confere.",
                "This device's certificate doesn't match.",
            )));
        }
        let agreement = hex::decode(&b.agreement_secret).unwrap_or_default();
        {
            let mut e = self.lock();
            e.create_linked_account(&b, &identity, device.clone(), &pending.relay)?;
            let peer = Peer {
                device: b.primary.clone(),
                their_hpke: b.primary_hpke.clone(),
                my_hpke_secret: pending.hpke_secret.clone(),
                secret: pending.secret.clone(),
            };
            let mut peers = load_peers(&vault);
            peers.push(peer.clone());
            if !vault.save(VAULT_IDENTITY.into(), identity.secret().to_vec())
                || !vault.save(VAULT_DEVICE.into(), device.secret().to_vec())
                || !vault.save(VAULT_AGREEMENT.into(), agreement.clone())
                || !save_peers(&vault, &peers)
            {
                let _ = e.wipe();
                let _ = e.store.meta_delete("account");
                return Err(invalid(t(
                    "Não deu para guardar a chave no Keychain.",
                    "Couldn't store the key in the Keychain.",
                )));
            }
            e.add_peer(peer);
            e.unlock_profile(Some(agreement))?;
        }
        vault.delete(link::VAULT_PENDING.into());
        self.account_dto_pub().ok_or_else(|| invalid("account"))
    }
}

#[cfg(test)]
mod pin_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[derive(Default)]
    struct Vault {
        reads: AtomicUsize,
        writes: AtomicUsize,
    }
    impl SecretVault for Vault {
        fn load(&self, _: String) -> Option<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            None
        }
        fn save(&self, _: String, _: Vec<u8>) -> bool {
            self.writes.fetch_add(1, Ordering::SeqCst);
            true
        }
        fn delete(&self, _: String) {
            self.writes.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn provider_identity_mismatch_cannot_install_a_decrypted_link_or_write_secrets() {
        let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
        let vault = Arc::new(Vault::default());
        let wrong = Signer::from_secret(&[91; 32]);
        let expected = ExpectedIdentity::parse(Some(Signer::from_secret(&[92; 32]).id())).unwrap();
        let device = Signer::from_secret(&[93; 32]);
        let pending = Pending {
            device_secret: hex::encode(device.secret()),
            hpke_secret: String::new(),
            hpke_public: String::new(),
            secret: hex::encode([94; 32]),
            relay: "http://localhost".into(),
        };
        let boxed = IdentityBox {
            identity_secret: hex::encode(wrong.secret()),
            agreement_secret: String::new(),
            cert: wrong.sign(&event_log::device_cert_message(&device.id())),
            name: "Wrong account".into(),
            handle: "wrong_account".into(),
            relay: pending.relay.clone(),
            primary: wrong.id(),
            primary_hpke: String::new(),
            profile_key: String::new(),
            profile_key_version: 0,
            profile_seen_version: 0,
            profile_fields: String::new(),
        };
        assert!(core
            .finish_link(&pending, &device, boxed, vault.clone(), &expected)
            .is_err());
        assert!(core.account().is_none());
        assert!(core.lock().store.meta("account").unwrap().is_none());
        assert_eq!(vault.reads.load(Ordering::SeqCst), 0);
        assert_eq!(vault.writes.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn malformed_provider_identity_is_refused_before_vault_reads_or_recovery_network() {
        let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
        let vault = Arc::new(Vault::default());
        assert!(core
            .link_wait_expected(vault.clone(), 1, Some("invalid".into()))
            .await
            .is_err());
        assert!(core
            .restore_backup_expected(
                "not a URL".into(),
                "valid_handle".into(),
                "secret".into(),
                vault.clone(),
                false,
                Some("invalid".into())
            )
            .await
            .is_err());
        assert!(core.account().is_none());
        assert_eq!(vault.reads.load(Ordering::SeqCst), 0);
        assert_eq!(vault.writes.load(Ordering::SeqCst), 0);
    }
}

#[cfg(test)]
mod initial_history_tests {
    use super::*;
    use event_log::{Author, SpaceLog};
    use std::{collections::HashMap, sync::Mutex};
    use types::{EventBody, Privacy, SpaceKind};

    #[derive(Default)]
    struct Vault(Mutex<HashMap<String, Vec<u8>>>);
    impl SecretVault for Vault {
        fn load(&self, key: String) -> Option<Vec<u8>> {
            self.0.lock().unwrap().get(&key).cloned()
        }
        fn save(&self, key: String, value: Vec<u8>) -> bool {
            self.0.lock().unwrap().insert(key, value);
            true
        }
        fn delete(&self, key: String) {
            self.0.lock().unwrap().remove(&key);
        }
    }
    fn install_link(core: &Client) -> Arc<Vault> {
        let vault = Arc::new(Vault::default());
        finish_test_link(core, vault.clone()).unwrap();
        vault
    }
    fn finish_test_link(
        core: &Client,
        vault: Arc<dyn SecretVault>,
    ) -> Result<AccountDto, CoreError> {
        let identity = Signer::from_secret(&[21; 32]);
        let device = Signer::from_secret(&[22; 32]);
        let (hpke_secret, hpke_public) = link::key_pair();
        let (_, primary_hpke) = link::key_pair();
        let pending = Pending {
            device_secret: hex::encode(device.secret()),
            hpke_secret: hex::encode(hpke_secret),
            hpke_public: hex::encode(hpke_public),
            secret: hex::encode([23; 32]),
            relay: "http://localhost:8787".into(),
        };
        let boxed = IdentityBox {
            identity_secret: hex::encode(identity.secret()),
            agreement_secret: hex::encode([24; 32]),
            cert: identity.sign(&event_log::device_cert_message(&device.id())),
            name: "Linked person".into(),
            handle: "linked_person".into(),
            relay: pending.relay.clone(),
            primary: Signer::from_secret(&[25; 32]).id(),
            primary_hpke: hex::encode(primary_hpke),
            profile_key: String::new(),
            profile_key_version: 0,
            profile_seen_version: 0,
            profile_fields: String::new(),
        };
        core.finish_link(
            &pending,
            &device,
            boxed,
            vault,
            &ExpectedIdentity::parse(None).unwrap(),
        )
    }
    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "zoen-initial-history-{}.sqlite",
            types::new_id("test")
        ))
    }
    fn encrypted_record(core: &Client) -> types::Event {
        use event_log::content::{Sealed, SealedKind};
        let identity = Signer::from_secret(&[21; 32]);
        let author = Author::device(&identity, Signer::from_secret(&[26; 32]));
        let mut log = SpaceLog::new("sp_initial_history");
        let genesis = log
            .append(
                &identity,
                1,
                EventBody::SpaceCreated {
                    title: "Private".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
            )
            .clone();
        let mut opened = author.sign_event(
            log.space(),
            "msg_initial",
            2,
            log.head(),
            EventBody::MessagePosted {
                message: "msg_initial".into(),
                text: "Actual durable history".into(),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        );
        let (content, sig) = author.sign_sealed(
            log.space(),
            "msg_initial",
            2,
            log.head().as_ref(),
            Sealed::new(SealedKind::Application, 1, vec![1, 2, 3]),
        );
        let outer = event_log::event_from_content(
            content,
            sig,
            author.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .unwrap();
        let local = log.sequence(outer).clone();
        opened.seq = local.seq;
        opened.sealed_wire = Some(event_log::content_hash_of(&local));
        let mut e = core.lock();
        e.store.append_events(&[genesis, local]).unwrap();
        e.reload().unwrap();
        opened
    }

    fn owned_personal_spaces(core: &Client) -> Vec<String> {
        let e = core.lock();
        let me = e.me_id().unwrap();
        e.state
            .spaces
            .values()
            .filter(|space| {
                space.kind == SpaceKind::Personal
                    && space
                        .members
                        .iter()
                        .any(|(id, role)| id == &me && *role == types::Role::Owner)
            })
            .map(|space| space.id.clone())
            .collect()
    }

    struct PreferencesFile(std::path::PathBuf);
    impl Drop for PreferencesFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            for suffix in ["-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
            }
        }
    }

    async fn assert_linked_preferences_survive_history_and_reopen(legacy: bool) {
        let file = PreferencesFile(path());
        let core = Client::open(file.0.to_string_lossy().into(), "en-US".into()).unwrap();
        let vault = install_link(&core);
        let account = core.account().unwrap();
        let identity_key = vault.load(VAULT_IDENTITY.into());
        let device_key = vault.load(VAULT_DEVICE.into());
        if legacy {
            // Represents the persisted pre-fix linked database: identity/device and
            // history are valid, but no Personal log was ever installed. Only this
            // isolated fixture's local Personal rows are removed; no account/key,
            // target chat or relay event is rewritten.
            let personal = owned_personal_spaces(&core);
            let mut e = core.lock();
            for id in personal {
                e.store
                    .conn()
                    .execute("DELETE FROM events WHERE space = ?1", [&id])
                    .unwrap();
            }
            e.reload().unwrap();
        } else {
            assert_eq!(owned_personal_spaces(&core).len(), 1);
        }
        let opened = encrypted_record(&core);
        let receive = core.history_receive.begin(5_000).unwrap();
        assert_eq!(
            core.commit_initial_history(vec![opened], 1, 0, &receive)
                .unwrap()
                .messages,
            1
        );
        drop(receive);
        assert!(!core.history_status().unwrap().initial_pending);
        let target = "sp_initial_history".to_owned();
        let before = core.lock().space_summary(&target).unwrap();
        assert!(before.members.iter().any(|member| member.is_me));
        assert!(!before.archived && !before.pinned && !before.muted);
        assert_eq!(core.timeline(target.clone()).unwrap().len(), 1);
        if legacy {
            assert!(owned_personal_spaces(&core).is_empty());
            // Reject a nonexistent target before a repair can create a resource.
            let failure = core
                .client_request(
                    r#"{"op":"set_chat_prefs","space_id":"sp_missing","pinned":true,"muted":true,"archived":true}"#.into(),
                )
                .await
                .unwrap_err();
            assert!(matches!(failure, CoreError::NotFound { what } if what == "Space"));
            assert!(owned_personal_spaces(&core).is_empty());
        }
        let prefs = |archived| {
            serde_json::json!({
                "op": "set_chat_prefs",
                "space_id": target,
                "pinned": true,
                "muted": true,
                "archived": archived,
            })
            .to_string()
        };
        // This exact public JSON path used to fail on legacy linked accounts with
        // NotFound{what:"Personal Space"}, despite the authorized readable chat.
        assert_eq!(core.client_request(prefs(false)).await.unwrap(), "null");
        assert_eq!(core.client_request(prefs(true)).await.unwrap(), "null");
        let personal = owned_personal_spaces(&core);
        assert_eq!(personal.len(), 1);
        let personal = personal[0].clone();
        {
            let e = core.lock();
            let events = e.logs[&personal].events();
            assert_eq!(events.len(), 3, "one genesis and two preference changes");
            assert!(events.iter().all(|event| {
                event.author == account.identity_id
                    && event.device.as_deref() == Some(account.device_id.as_str())
                    && event.cert.is_some()
            }));
            assert!(matches!(
                events[0].body,
                EventBody::SpaceCreated {
                    kind: SpaceKind::Personal,
                    ..
                }
            ));
            let current = e.space_summary(&target).unwrap();
            assert!(current.archived && current.pinned && current.muted);
        }
        assert_eq!(core.client_request(prefs(true)).await.unwrap(), "null");
        assert_eq!(core.lock().logs[&personal].events().len(), 3);
        assert_eq!(owned_personal_spaces(&core), vec![personal.clone()]);
        drop(core);
        let reopened = Client::open(file.0.to_string_lossy().into(), "en-US".into()).unwrap();
        assert!(reopened.unlock(vault.clone()).unwrap());
        assert_eq!(reopened.account().unwrap().identity_id, account.identity_id);
        assert_eq!(vault.load(VAULT_IDENTITY.into()), identity_key);
        assert_eq!(vault.load(VAULT_DEVICE.into()), device_key);
        assert!(!reopened.history_status().unwrap().initial_pending);
        assert_eq!(owned_personal_spaces(&reopened), vec![personal.clone()]);
        let after = reopened.lock().space_summary(&target).unwrap();
        assert!(after.archived && after.pinned && after.muted);
        assert_eq!(reopened.timeline(target.clone()).unwrap().len(), 1);
        assert_eq!(reopened.client_request(prefs(true)).await.unwrap(), "null");
        assert_eq!(reopened.lock().logs[&personal].events().len(), 3);
    }

    #[tokio::test]
    async fn linked_chat_preferences_bootstrap_is_signed_idempotent_and_survives_history_reopen() {
        assert_linked_preferences_survive_history_and_reopen(false).await;
    }

    #[tokio::test]
    async fn linked_chat_preferences_repair_legacy_missing_personal_after_verified_history() {
        assert_linked_preferences_survive_history_and_reopen(true).await;
    }

    #[test]
    fn linked_chat_preferences_bootstrap_storage_failure_rolls_back_before_vault_and_can_retry() {
        let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
        let vault = Arc::new(Vault::default());
        core.lock()
            .store
            .conn()
            .execute_batch(
                "CREATE TEMP TRIGGER fail_link_bootstrap BEFORE INSERT ON events \
                 BEGIN SELECT RAISE(ABORT, 'fixture refused bootstrap'); END;",
            )
            .unwrap();
        let failure = finish_test_link(&core, vault.clone()).unwrap_err();
        assert!(matches!(failure, CoreError::Storage { .. }));
        assert!(core.account().is_none());
        assert!(core.lock().state.spaces.is_empty());
        {
            let e = core.lock();
            assert!(e.me.is_none());
            assert!(e.net.author.is_none());
            assert!(e.is_empty());
            assert_eq!(e.store.event_count().unwrap(), 0);
            assert!(e.store.meta("account").unwrap().is_none());
            assert!(e
                .store
                .meta(META_INITIAL_HISTORY_PENDING)
                .unwrap()
                .is_none());
            assert!(e.store.meta(META_PRIMARY).unwrap().is_none());
        }
        assert!(
            vault.0.lock().unwrap().is_empty(),
            "failed install never reaches vault writes"
        );
        core.lock()
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_link_bootstrap")
            .unwrap();
        finish_test_link(&core, vault.clone()).unwrap();
        assert_eq!(owned_personal_spaces(&core).len(), 1);
        assert!(core.account().is_some());
        assert!(vault.load(VAULT_IDENTITY.into()).is_some());
        assert!(vault.load(VAULT_DEVICE.into()).is_some());
    }

    #[tokio::test]
    async fn linked_chat_preferences_never_reuse_another_identity_personal_space() {
        let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
        let vault = install_link(&core);
        let identity_key = vault.load(VAULT_IDENTITY.into());
        let device_key = vault.load(VAULT_DEVICE.into());
        let old = owned_personal_spaces(&core);
        {
            let mut e = core.lock();
            for id in old {
                e.store
                    .conn()
                    .execute("DELETE FROM events WHERE space = ?1", [&id])
                    .unwrap();
            }
            e.reload().unwrap();
            let foreign = e
                .create_identity(
                    types::Identity {
                        id: String::new(),
                        kind: types::IdentityKind::Person,
                        name: "Other local fixture identity".into(),
                        handle: "other_fixture".into(),
                        tint_hex: "#123456".into(),
                        glyph: None,
                        owner: None,
                        bio: String::new(),
                        owner_proof: None,
                    },
                    true,
                )
                .unwrap();
            e.append(
                "sp_foreign_personal",
                &foreign,
                EventBody::SpaceCreated {
                    title: "Other Personal".into(),
                    kind: SpaceKind::Personal,
                    privacy: Privacy::EndToEnd,
                },
            )
            .unwrap();
        }
        let opened = encrypted_record(&core);
        let receive = core.history_receive.begin(5_000).unwrap();
        core.commit_initial_history(vec![opened], 1, 0, &receive)
            .unwrap();
        drop(receive);
        assert!(owned_personal_spaces(&core).is_empty());
        let foreign_before = core.lock().logs["sp_foreign_personal"].events().to_vec();
        let command = r#"{"op":"set_chat_prefs","space_id":"sp_initial_history","pinned":false,"muted":false,"archived":true}"#;
        assert_eq!(core.client_request(command.into()).await.unwrap(), "null");
        let personal = owned_personal_spaces(&core);
        assert_eq!(personal.len(), 1);
        assert_ne!(personal[0], "sp_foreign_personal");
        assert_eq!(
            core.lock().logs["sp_foreign_personal"].events(),
            foreign_before.as_slice()
        );
        let current = core.lock().space_summary("sp_initial_history").unwrap();
        assert!(current.archived);
        assert!(!current.pinned && !current.muted);
        assert_eq!(vault.load(VAULT_IDENTITY.into()), identity_key);
        assert_eq!(vault.load(VAULT_DEVICE.into()), device_key);
    }

    #[test]
    fn linked_chat_preferences_vault_rejection_cleans_the_new_bootstrap() {
        struct RejectingVault;
        impl SecretVault for RejectingVault {
            fn load(&self, _: String) -> Option<Vec<u8>> {
                None
            }
            fn save(&self, _: String, _: Vec<u8>) -> bool {
                false
            }
            fn delete(&self, _: String) {}
        }
        let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
        let failure = finish_test_link(&core, Arc::new(RejectingVault)).unwrap_err();
        assert!(matches!(failure, CoreError::Invalid { .. }));
        assert!(core.account().is_none());
        {
            let e = core.lock();
            assert!(e.me.is_none());
            assert!(e.net.author.is_none());
            assert!(e.state.spaces.is_empty());
            assert!(e.is_empty());
            assert_eq!(e.store.event_count().unwrap(), 0);
        }
        install_link(&core);
        assert_eq!(owned_personal_spaces(&core).len(), 1);
    }

    #[tokio::test]
    async fn link_then_cold_reopen_before_history_retains_pending_and_same_keys() {
        let file = path();
        let core = Client::open(file.to_string_lossy().into(), "en-US".into()).unwrap();
        let vault = install_link(&core);
        let account = core.account().unwrap();
        let identity_key = vault.load(VAULT_IDENTITY.into());
        let device_key = vault.load(VAULT_DEVICE.into());
        assert!(core.history_status().unwrap().initial_pending);
        drop(core);
        let reopened = Client::open(file.to_string_lossy().into(), "en-US".into()).unwrap();
        assert!(reopened.unlock(vault.clone()).unwrap());
        assert_eq!(reopened.account().unwrap().identity_id, account.identity_id);
        let status = reopened.history_status().unwrap();
        assert!(status.initial_pending);
        assert!(!status.manifest_received);
        assert!(reopened.receive_history(20, None).await.is_err());
        assert!(reopened.history_status().unwrap().initial_pending);
        assert_eq!(vault.load(VAULT_IDENTITY.into()), identity_key);
        assert_eq!(vault.load(VAULT_DEVICE.into()), device_key);
        drop(reopened);
        let _ = std::fs::remove_file(file);
    }

    #[tokio::test]
    async fn missing_or_forged_history_cannot_clear_pending_or_partially_import() {
        let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
        install_link(&core);
        let opened = encrypted_record(&core);
        let receive = core.history_receive.begin(5_000).unwrap();
        let mut missing = opened.clone();
        missing.space = "sp_not_synced".into();
        assert!(core
            .commit_initial_history(vec![opened.clone(), missing], 1, 0, &receive)
            .is_err());
        assert!(core.history_status().unwrap().initial_pending);
        assert!(matches!(
            core.lock().logs["sp_initial_history"].events()[1].body,
            EventBody::Sealed { .. }
        ));
        assert!(core
            .lock()
            .store
            .meta(META_HISTORY_COMPLETE)
            .unwrap()
            .is_none());
        let mut forged = opened.clone();
        forged.sig = "00".repeat(64);
        assert!(core
            .commit_initial_history(vec![forged], 1, 0, &receive)
            .is_err());
        assert!(core.history_status().unwrap().initial_pending);
        assert_eq!(
            core.commit_initial_history(vec![opened], 1, 0, &receive)
                .unwrap()
                .messages,
            1
        );
        assert!(!core.history_status().unwrap().initial_pending);
        assert!(matches!(
            core.lock().logs["sp_initial_history"].events()[1].body,
            EventBody::MessagePosted { .. }
        ));
    }

    #[tokio::test]
    async fn durable_import_and_receipt_survive_reopen_before_relay_cleanup_or_ui_ack() {
        let file = path();
        let core = Client::open(file.to_string_lossy().into(), "en-US".into()).unwrap();
        let vault = install_link(&core);
        let opened = encrypted_record(&core);
        let receive = core.history_receive.begin(5_000).unwrap();
        core.lock()
            .store
            .set_meta(META_HISTORY, r#"{"transfer":"old"}"#)
            .unwrap();
        core.lock()
            .store
            .set_meta(&chunk_meta(0), "retained sealed bytes")
            .unwrap();
        let receipt = core
            .commit_initial_history(vec![opened], 1, 1, &receive)
            .unwrap();
        drop(receive);
        drop(core);
        let reopened = Client::open(file.to_string_lossy().into(), "en-US".into()).unwrap();
        assert!(reopened.unlock(vault).unwrap());
        assert!(!reopened.history_status().unwrap().initial_pending);
        assert!(reopened.lock().store.meta(META_HISTORY).unwrap().is_none());
        assert!(reopened
            .lock()
            .store
            .meta(&chunk_meta(0))
            .unwrap()
            .is_none());
        // No relay connection exists: a successful replay must use the committed receipt.
        assert_eq!(
            reopened.receive_history(5_000, None).await.unwrap(),
            receipt
        );
        assert!(matches!(
            reopened.lock().logs["sp_initial_history"].events()[1].body,
            EventBody::MessagePosted { .. }
        ));
        drop(reopened);
        let _ = std::fs::remove_file(file);
    }

    #[tokio::test]
    async fn cancellation_deadline_and_single_receiver_guard_leave_pending_retryable() {
        let control = Arc::new(HistoryControl::default());
        let mut receive = control.begin(5_000).unwrap();
        assert!(control.begin(5_000).is_err());
        let cancel = control.clone();
        let cancellation = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel.cancel();
        });
        assert!(receive
            .wait(std::future::pending::<Result<(), CoreError>>())
            .await
            .is_err());
        cancellation.await.unwrap();
        drop(receive);
        let mut retry = control.begin(10).unwrap();
        assert!(retry
            .wait(std::future::pending::<Result<(), CoreError>>())
            .await
            .is_err());
        drop(retry);
        assert!(control.begin(5_000).is_ok());
        assert!(!valid_history_manifest(&HistoryBox {
            transfer: "a".repeat(64),
            key: "b".repeat(64),
            chunks: vec![]
        }));
    }

    #[test]
    fn history_ready_is_a_content_free_authenticated_primary_hint_only_while_pending() {
        let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
        install_link(&core);
        let primary = core.lock().store.meta(META_PRIMARY).unwrap().unwrap();
        let recipient = core.account().unwrap().device_id;
        let (recipient_secret, recipient_public) = link::key_pair();
        let (sender_secret, sender_public) = link::key_pair();
        let secret = hex::encode([41; 32]);
        core.lock().add_peer(Peer {
            device: primary.clone(),
            their_hpke: hex::encode(sender_public),
            my_hpke_secret: hex::encode(recipient_secret),
            secret: secret.clone(),
        });
        let mut sender = crate::Engine::open(":memory:").unwrap();
        sender.add_peer(Peer {
            device: recipient.clone(),
            their_hpke: hex::encode(recipient_public),
            my_hpke_secret: hex::encode(sender_secret),
            secret,
        });
        sender.announce_initial_history(&recipient);
        let queued = sender.take_device_outbox();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].0, recipient);
        let sealed = &queued[0].1;
        assert!(!String::from_utf8_lossy(sealed).contains("HistoryReady"));
        {
            let mut e = core.lock();
            e.device_message("unknown", &recipient, sealed);
            assert!(!e.take_initial_history_ready());
            e.device_message(&primary, "another recipient", sealed);
            assert!(!e.take_initial_history_ready());
            let mut corrupt = sealed.clone();
            *corrupt.last_mut().unwrap() ^= 1;
            e.device_message(&primary, &recipient, &corrupt);
            assert!(!e.take_initial_history_ready());
            e.device_message(&primary, &recipient, sealed);
            assert!(e.take_initial_history_ready());
            assert!(!e.take_initial_history_ready());
            e.store
                .set_meta("devices.revoked", &serde_json::json!([primary]).to_string())
                .unwrap();
            e.device_message(&primary, &recipient, sealed);
            assert!(!e.take_initial_history_ready());
            e.store.meta_delete("devices.revoked").unwrap();
            e.store.meta_delete(META_INITIAL_HISTORY_PENDING).unwrap();
            e.device_message(&primary, &recipient, sealed);
            assert!(!e.take_initial_history_ready());
        }
    }
}
