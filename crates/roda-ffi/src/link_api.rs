//! Linking a second device, from the apps and the CLI (ADR 0045).
//!
//! New device: [`RodaEngine::link_request`] (shows the code), [`RodaEngine::link_wait`]
//! (becomes the account), then [`RodaEngine::receive_history`] (the first bundle) and
//! [`RodaEngine::load_older`] when the person scrolls up. Existing device:
//! [`RodaEngine::link_device`], then [`RodaEngine::send_history`] once the new device is in
//! every group.

use std::{sync::Arc, time::Duration};

use base64::Engine as _;
use roda_log::Signer;
use roda_proto::{transfer_message, Op, Reply};

use crate::{
    api::SecretVault,
    i18n::t,
    link::{self, HistoryBox, IdentityBox, LinkCode, Peer, Pending},
    linking::META_PRIMARY,
    net::http_base,
    profile::VAULT_AGREEMENT,
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    AccountDto, CoreError, RodaEngine,
};

const VAULT_PEERS: &str = "link.peers";
const META_HISTORY: &str = "link.history";

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

/// Progress of a history transfer, chunk by chunk.
#[uniffi::export(with_foreign)]
pub trait TransferListener: Send + Sync {
    fn on_progress(&self, done: u32, total: u32);
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LinkRequestDto {
    /// What the QR code carries.
    pub code: String,
    /// Six digits both screens show.
    pub check: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LinkedDto {
    pub device_id: String,
    pub check: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HistoryDto {
    pub messages: u32,
    pub chunks: u32,
    /// Chunks already here from an earlier, interrupted try.
    pub resumed: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OlderDto {
    pub loaded: u32,
    /// Older messages are left on the other device.
    pub more: bool,
    /// The device that has them didn't answer: it's offline.
    pub primary_offline: bool,
    /// What to show when it is offline.
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DeviceDto {
    pub device_id: String,
    pub revoked: bool,
    pub this_device: bool,
}

fn http() -> Result<reqwest::Client, CoreError> {
    reqwest::Client::builder()
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

impl RodaEngine {
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
impl RodaEngine {
    /// New device: makes the code to show (as a QR) and keeps what it needs to finish.
    pub fn link_request(
        &self,
        relay_url: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<LinkRequestDto, CoreError> {
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
    }

    /// Existing device: devices of this account, as the relay lists them.
    pub fn link_code_check(&self, code: String) -> Result<String, CoreError> {
        LinkCode::parse(&code)
            .map(|c| c.check_digits())
            .ok_or_else(|| invalid(t("Código inválido.", "Invalid code.")))
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl RodaEngine {
    /// New device: waits for the existing device's box, checks it and becomes the account.
    /// Start sync afterwards.
    pub async fn link_wait(
        &self,
        vault: Arc<dyn SecretVault>,
        timeout_ms: u64,
    ) -> Result<AccountDto, CoreError> {
        let pending: Pending = vault
            .load(link::VAULT_PENDING.into())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or_else(|| invalid(t("Nenhum pedido de vínculo.", "No link request.")))?;
        let device = Signer::from_secret(
            &secret32(hex::decode(&pending.device_secret).ok())
                .ok_or_else(|| invalid("link request"))?,
        );
        let secret =
            secret32(hex::decode(&pending.secret).ok()).ok_or_else(|| invalid("link request"))?;
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        let sealed = crate::net::fetch_link_box(
            &pending.relay,
            &device,
            &link::box_id(&secret, "identity"),
            deadline,
        )
        .await
        .map_err(invalid)?;
        let hpke_secret = hex::decode(&pending.hpke_secret).unwrap_or_default();
        let plain = link::open(&hpke_secret, &secret, "identity", &sealed)
            .ok_or_else(|| invalid(t("O vínculo não abriu.", "The link didn't open.")))?;
        let b: IdentityBox = serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
        let identity = Signer::from_secret(
            &secret32(hex::decode(&b.identity_secret).ok()).ok_or_else(|| invalid("identity"))?,
        );
        // The certificate must be the account's, for this device.
        if !roda_log::verify_sig(
            &identity.id(),
            &roda_log::device_cert_message(&device.id()),
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
                cert: identity.sign(&roda_log::device_cert_message(&code.device)),
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
            let sha = roda_log::sha256_hex(&sealed);
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
            })
            .await?
        {
            Reply::Done => Ok(bundle.events.len() as u32),
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
        let manifest = self.history_manifest(timeout_ms).await?;
        let key = secret32(hex::decode(&manifest.key).ok()).ok_or_else(|| invalid("history"))?;
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
            if self.lock().store.meta(&chunk_meta(n))?.is_some() {
                resumed += 1;
                continue;
            }
            if stop_after.is_some_and(|s| fetched >= s) {
                return Err(invalid(format!("history stopped at {}/{total}", n)));
            }
            let r = client
                .get(format!("{base}/v1/transfer/{}/{n}", manifest.transfer))
                .send()
                .await
                .map_err(|e| invalid(e.to_string()))?;
            if !r.status().is_success() {
                return Err(invalid(format!("history download: {}", r.status())));
            }
            let bytes = r.bytes().await.map_err(|e| invalid(e.to_string()))?;
            if roda_log::sha256_hex(&bytes) != *sha {
                return Err(invalid("a history chunk doesn't match"));
            }
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
            let sealed = self
                .lock()
                .store
                .meta(&chunk_meta(n))?
                .and_then(|s| b64.decode(s).ok())
                .ok_or_else(|| invalid("history chunk"))?;
            plain.extend(
                link::open_chunk(&key, n as u32, &sealed)
                    .ok_or_else(|| invalid("a history chunk doesn't open"))?,
            );
        }
        let bundle: link::Bundle =
            serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
        let messages = self.lock().import_opened(bundle.events)?;
        // Gone from the relay once it's here.
        let (device, ts, sig) =
            self.signed_headers(|ts| transfer_message("delete", &manifest.transfer, "", "", ts))?;
        let _ = client
            .delete(format!("{base}/v1/transfer/{}", manifest.transfer))
            .header("x-zoen-device", device)
            .header("x-zoen-ts", ts)
            .header("x-zoen-sig", sig)
            .send()
            .await;
        {
            let e = self.lock();
            for n in 0..total {
                let _ = e.store.meta_delete(&chunk_meta(n));
            }
            let _ = e.store.meta_delete(META_HISTORY);
        }
        Ok(HistoryDto {
            messages,
            chunks: total as u32,
            resumed,
        })
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

impl RodaEngine {
    /// The history box: from the device database if an earlier try got it, else from the
    /// relay (one-time), kept until the bundle is in.
    async fn history_manifest(&self, timeout_ms: u64) -> Result<HistoryBox, CoreError> {
        if let Some(m) = self
            .lock()
            .store
            .meta(META_HISTORY)?
            .and_then(|v| serde_json::from_str(&v).ok())
        {
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
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        let sealed = loop {
            match self.request(Op::FetchLink { id: id.clone() }).await? {
                Reply::Link(Some(b)) => break b,
                Reply::Link(None) => {}
                _ => return Err(invalid("the relay answered something else")),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(invalid(t(
                    "O histórico ainda não chegou.",
                    "The history hasn't arrived yet.",
                )));
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
        };
        let sk = hex::decode(&peer.my_hpke_secret).unwrap_or_default();
        let plain = link::open(&sk, &secret, "history", &sealed)
            .ok_or_else(|| invalid(t("O histórico não abriu.", "The history didn't open.")))?;
        let m: HistoryBox = serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
        self.lock()
            .store
            .set_meta(META_HISTORY, &serde_json::to_string(&m).unwrap_or_default())?;
        Ok(m)
    }
}
