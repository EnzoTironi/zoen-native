//! Encrypted backup in the app API (ADR 0046): turn it on with a password or a recovery
//! key, back up now, turn it off, and restore on a new device.

use std::{sync::Arc, time::Duration};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use roda_proto::backup_message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    api::{AccountDto, SecretVault},
    backup::{self, Header, Kdf, Keys, VAULT_BACKUP, VAULT_BACKUP_GENERATION},
    i18n::t,
    net::http_base,
    profile::VAULT_AGREEMENT,
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    CoreError, RodaEngine,
};

const STATUS_META: &str = "backup.status";

#[derive(Debug, Clone, PartialEq, uniffi::Record, Serialize, Deserialize, Default)]
pub struct BackupStatusDto {
    pub enabled: bool,
    /// "password" or "recovery_key" (empty while off).
    pub mode: String,
    /// When the last backup reached the server (ms since 1970; 0 = never).
    pub last_backup_ms: i64,
    pub bytes: u64,
}

fn invalid(reason: impl Into<String>) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}

fn portuguese_of(engine: &RodaEngine) -> bool {
    engine.language() != "en"
}

fn say(portuguese: bool, pt: &str, en: &str) -> String {
    if portuguese {
        pt.to_string()
    } else {
        en.to_string()
    }
}

fn account_full(portuguese: bool) -> CoreError {
    invalid(say(
        portuguese,
        "Este backup não cabe na conta.",
        "This backup does not fit this account.",
    ))
}

fn offline(e: impl std::fmt::Display) -> CoreError {
    invalid(format!(
        "{} ({e})",
        t(
            "Sem conexão com o servidor agora.",
            "Can't reach the server right now."
        )
    ))
}

fn offline_in(portuguese: bool, e: impl std::fmt::Display) -> CoreError {
    invalid(format!(
        "{} ({e})",
        say(
            portuguese,
            "Sem conexão com o servidor agora.",
            "Can't reach the server right now.",
        )
    ))
}

fn http() -> Result<reqwest::Client, CoreError> {
    // Restore runs before any sync session has installed the TLS provider.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(offline)
}

/// Turns a relay refusal into words for people. A 413 never forwards the raw body.
async fn refused(r: reqwest::Response, portuguese: bool) -> CoreError {
    let status = r.status().as_u16();
    let body = r.text().await.unwrap_or_default();
    match status {
        403 if body.contains("device") => invalid(say(
            portuguese,
            "Este aparelho foi desvinculado. Use um aparelho autorizado.",
            "This device was unlinked. Use an authorized device.",
        )),
        403 => invalid(say(
            portuguese,
            "Senha ou chave de recuperação errada.",
            "Wrong password or recovery key.",
        )),
        413 => account_full(portuguese),
        423 => invalid(say(
            portuguese,
            "Muitas tentativas erradas. Este backup não pode mais ser aberto.",
            "Too many wrong attempts. This backup can no longer be opened.",
        )),
        404 => invalid(say(
            portuguese,
            "Não encontramos um backup para esse @.",
            "We couldn't find a backup for that @.",
        )),
        429 => invalid(say(
            portuguese,
            "Muitas tentativas. Espere um pouco e tente de novo.",
            "Too many attempts. Wait a bit and try again.",
        )),
        503 if body.contains("not configured") => invalid(say(
            portuguese,
            "Backup com senha ainda não está disponível neste servidor.",
            "Password backup isn't available on this server yet.",
        )),
        _ => invalid(format!("backup: {status} {body}")),
    }
}

/// What a signed backup write needs, read under the lock and released before any await.
struct Creds {
    base: String,
    relay_name: String,
    identity: String,
    key: roda_log::Signer,
    portuguese: bool,
}

async fn signed(
    http: &reqwest::Client,
    c: &Creds,
    method: reqwest::Method,
    path: &str,
    op: &str,
    body: Vec<u8>,
) -> Result<reqwest::Response, CoreError> {
    let response = send_raw(http, c, method, path, op, body, "application/json").await?;
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(refused(response, c.portuguese).await)
    }
}

async fn send_raw(
    http: &reqwest::Client,
    c: &Creds,
    method: reqwest::Method,
    path: &str,
    op: &str,
    body: Vec<u8>,
    content_type: &str,
) -> Result<reqwest::Response, CoreError> {
    let ts = crate::engine::now_ms();
    let sha = hex::encode(Sha256::digest(&body));
    let sig = c
        .key
        .sign(&backup_message(&c.relay_name, &c.identity, op, &sha, ts));
    http.request(method, format!("{}{path}", c.base))
        .header("x-zoen-device", c.key.id())
        .header("x-zoen-ts", ts.to_string())
        .header("x-zoen-sig", sig)
        .header("content-type", content_type)
        .body(body)
        .send()
        .await
        .map_err(|error| offline_in(c.portuguese, error))
}

#[derive(Deserialize)]
struct Evaluated {
    evaluated: String,
}

#[derive(Deserialize)]
struct Started {
    identity: String,
    mode: String,
    kdf: serde_json::Value,
    evaluated: Option<String>,
    generation: Option<String>,
}

#[derive(Deserialize)]
struct Opened {
    wrapped_key: String,
    generation: Option<String>,
}

async fn json<T: serde::de::DeserializeOwned>(r: reqwest::Response) -> Result<T, CoreError> {
    let b = r.bytes().await.map_err(offline)?;
    serde_json::from_slice(&b).map_err(|_| invalid("backup: bad answer from the server"))
}

fn hex32(s: &str) -> Result<[u8; 32], CoreError> {
    hex::decode(s)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| invalid("backup: bad answer from the server"))
}

const CHUNK_BYTES: usize = 8 * 1024 * 1024;
const PLAIN_CEILING: usize = 1024 * 1024 * 1024;
const COMPRESS_MAGIC: &[u8] = b"ZOENZ1\0";
const PENDING_MAGIC: &[u8; 8] = b"ZOENBP1\0";
const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";
const WINDOW: usize = 32 * 1024;
const MIN_MATCH: usize = 4;
const MAX_MATCH: usize = 255;
const SLOTS: usize = 1 << 20;

enum Prepared {
    Resume {
        sealed: Vec<u8>,
        db_path: String,
    },
    Fresh {
        header: Header,
        db: Vec<u8>,
        snapshot: [u8; 32],
        db_path: String,
    },
    Changed {
        db_path: String,
    },
    Broken {
        db_path: String,
    },
}

struct Pending {
    generation: [u8; 32],
    snapshot: [u8; 32],
    sealed: Vec<u8>,
}

enum UploadErr {
    Again,
    Stop(CoreError),
}

#[derive(Deserialize)]
struct ChunkPlan {
    ceiling: u64,
    missing: Vec<u32>,
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Debug)]
enum Decision {
    Resume(Vec<u8>),
    Fresh,
    Changed,
    Broken,
}

fn decide(
    loaded: Result<Option<Pending>, ()>,
    generation: &[u8; 32],
    snapshot: &[u8; 32],
) -> Decision {
    match loaded {
        Ok(Some(pending)) if pending.generation == *generation && pending.snapshot == *snapshot => {
            Decision::Resume(pending.sealed)
        }
        Ok(Some(_)) => Decision::Changed,
        Ok(None) => Decision::Fresh,
        Err(()) => Decision::Broken,
    }
}

fn shows_sqlite(bytes: &[u8]) -> bool {
    bytes
        .windows(SQLITE_HEADER.len())
        .any(|window| window == SQLITE_HEADER)
}

fn corrupted(portuguese: bool) -> CoreError {
    invalid(say(
        portuguese,
        "O backup está corrompido.",
        "The backup is corrupted.",
    ))
}

fn history_changed(portuguese: bool) -> CoreError {
    invalid(say(
        portuguese,
        "O histórico mudou desde o backup interrompido. Rode o backup de novo para enviar o histórico de agora.",
        "The history changed since the interrupted backup. Run the backup again to send the history as it is now.",
    ))
}

fn pending_unreadable(portuguese: bool) -> CoreError {
    invalid(say(
        portuguese,
        "O backup interrompido não pôde ser lido. Rode o backup de novo para começar outro.",
        "The interrupted backup could not be read. Run the backup again to start another.",
    ))
}

fn stopped(portuguese: bool, piece: u32, total: u32) -> CoreError {
    invalid(say(
        portuguese,
        &format!(
            "O backup parou no pedaço {piece} de {total}. O que já foi confirmado continua. Rode o backup de novo para continuar desse pedaço."
        ),
        &format!(
            "The backup stopped at piece {piece} of {total}. What is already confirmed stays. Run the backup again to continue from that piece."
        ),
    ))
}

fn piece_count(total: usize) -> Option<u32> {
    if total == 0 {
        return None;
    }
    u32::try_from(total.div_ceil(CHUNK_BYTES)).ok()
}

fn piece_at(sealed: &[u8], index: u32) -> &[u8] {
    let start = index as usize * CHUNK_BYTES;
    let end = (start + CHUNK_BYTES).min(sealed.len());
    &sealed[start..end]
}

fn confirmed_bytes(total: usize, missing: &[u32]) -> u64 {
    let mut absent = 0usize;
    for index in missing {
        let start = *index as usize * CHUNK_BYTES;
        if start >= total {
            continue;
        }
        absent += (total - start).min(CHUNK_BYTES);
    }
    total.saturating_sub(absent) as u64
}

fn frame(generation: &[u8; 32], index: u32, count: u32, total: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(56 + payload.len());
    out.extend_from_slice(b"ZOENBC1\0");
    out.extend_from_slice(generation);
    out.extend_from_slice(&index.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&total.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn pending_path(db_path: &str) -> Option<std::path::PathBuf> {
    if db_path.is_empty() || db_path == ":memory:" {
        None
    } else {
        Some(std::path::PathBuf::from(format!(
            "{db_path}.zoen-backup-pending"
        )))
    }
}

fn clear_pending(db_path: &str) {
    if let Some(path) = pending_path(db_path) {
        let _ = std::fs::remove_file(path);
    }
}

fn save_pending(db_path: &str, pending: &Pending) -> Result<(), CoreError> {
    let Some(path) = pending_path(db_path) else {
        return Ok(());
    };
    let mut bytes = Vec::with_capacity(104 + pending.sealed.len());
    bytes.extend_from_slice(PENDING_MAGIC);
    bytes.extend_from_slice(&pending.generation);
    bytes.extend_from_slice(&pending.snapshot);
    bytes.extend_from_slice(&sha256(&pending.sealed));
    bytes.extend_from_slice(&pending.sealed);
    let tmp = std::path::PathBuf::from(format!("{}.tmp", path.display()));
    std::fs::write(&tmp, &bytes).map_err(|error| invalid(error.to_string()))?;
    #[cfg(unix)]
    {
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &path).map_err(|error| invalid(error.to_string()))?;
    Ok(())
}

fn load_pending(db_path: &str) -> Result<Option<Pending>, ()> {
    let Some(path) = pending_path(db_path) else {
        return Ok(None);
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    if bytes.len() < 104 || bytes.get(..8) != Some(PENDING_MAGIC.as_slice()) {
        return Err(());
    }
    let generation = bytes[8..40].try_into().map_err(|_| ())?;
    let snapshot = bytes[40..72].try_into().map_err(|_| ())?;
    let digest = &bytes[72..104];
    let sealed = bytes[104..].to_vec();
    if sha256(&sealed).as_slice() != digest {
        return Err(());
    }
    Ok(Some(Pending {
        generation,
        snapshot,
        sealed,
    }))
}

#[derive(Clone, Copy)]
struct Slot {
    key: u32,
    pos: u32,
}

fn remember(slots: &mut [Slot], data: &[u8], index: usize) {
    if index + 2 >= data.len() || index > u32::MAX as usize {
        return;
    }
    let key =
        ((data[index] as u32) << 16) | ((data[index + 1] as u32) << 8) | data[index + 2] as u32;
    let slot = (key as usize).wrapping_mul(0x9E37_79B1) & (slots.len() - 1);
    slots[slot] = Slot {
        key,
        pos: index as u32,
    };
}

fn find_match(slots: &[Slot], data: &[u8], index: usize) -> Option<(u16, u8)> {
    if index + MIN_MATCH > data.len() {
        return None;
    }
    let key =
        ((data[index] as u32) << 16) | ((data[index + 1] as u32) << 8) | data[index + 2] as u32;
    let slot = &slots[(key as usize).wrapping_mul(0x9E37_79B1) & (slots.len() - 1)];
    if slot.pos == u32::MAX || slot.key != key {
        return None;
    }
    let prev = slot.pos as usize;
    if prev >= index {
        return None;
    }
    let distance = index - prev;
    if distance == 0 || distance > WINDOW || distance > u16::MAX as usize {
        return None;
    }
    if data[prev..prev + 3] != data[index..index + 3] {
        return None;
    }
    let max = MAX_MATCH.min(data.len() - index);
    let mut length = 3;
    while length < max && data[index + length] == data[index + length - distance] {
        length += 1;
    }
    if length < MIN_MATCH {
        return None;
    }
    Some((distance as u16, length as u8))
}

fn compress(input: &[u8]) -> Vec<u8> {
    let mut slots = vec![
        Slot {
            key: 0,
            pos: u32::MAX,
        };
        SLOTS
    ];
    let mut out = Vec::with_capacity(16 + input.len() + input.len() / 8);
    out.extend_from_slice(COMPRESS_MAGIC);
    out.extend_from_slice(&(input.len() as u64).to_be_bytes());
    let mut index = 0;
    while index < input.len() {
        let flag_at = out.len();
        out.push(0);
        let mut flag = 0u8;
        for bit in 0..8 {
            if index >= input.len() {
                break;
            }
            if let Some((distance, length)) = find_match(&slots, input, index) {
                flag |= 1 << bit;
                let length = length as usize;
                for position in index..index + length {
                    remember(&mut slots, input, position);
                }
                out.extend_from_slice(&distance.to_be_bytes());
                out.push(length as u8);
                index += length;
            } else {
                remember(&mut slots, input, index);
                out.push(input[index]);
                index += 1;
            }
        }
        out[flag_at] = flag;
    }
    out
}

fn decompress(input: &[u8]) -> Result<Vec<u8>, ()> {
    if input.len() < 15 || input.get(..COMPRESS_MAGIC.len()) != Some(COMPRESS_MAGIC) {
        return Err(());
    }
    let original = u64::from_be_bytes(input[7..15].try_into().map_err(|_| ())?);
    if original > PLAIN_CEILING as u64 {
        return Err(());
    }
    let mut cursor = 15;
    let mut out = Vec::new();
    while out.len() < original as usize {
        if cursor >= input.len() {
            return Err(());
        }
        let flag = input[cursor];
        cursor += 1;
        for bit in 0..8 {
            if out.len() == original as usize {
                break;
            }
            if flag & (1 << bit) == 0 {
                if cursor >= input.len() {
                    return Err(());
                }
                out.push(input[cursor]);
                cursor += 1;
                continue;
            }
            if cursor + 3 > input.len() {
                return Err(());
            }
            let distance =
                u16::from_be_bytes(input[cursor..cursor + 2].try_into().map_err(|_| ())?) as usize;
            let length = input[cursor + 2] as usize;
            cursor += 3;
            if distance == 0 || distance > out.len() || !(MIN_MATCH..=MAX_MATCH).contains(&length) {
                return Err(());
            }
            if out.len() + length > original as usize {
                return Err(());
            }
            for _ in 0..length {
                let byte = out[out.len() - distance];
                out.push(byte);
            }
        }
    }
    if cursor != input.len() || out.len() != original as usize {
        return Err(());
    }
    Ok(out)
}

fn retry_after(response: &reqwest::Response) -> u64 {
    response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(1)
        .clamp(1, 30)
}

async fn fetch_plan(
    http: &reqwest::Client,
    creds: &Creds,
    generation: &[u8; 32],
    total: u64,
    chunks: u32,
    sha: &str,
) -> Result<ChunkPlan, UploadErr> {
    let portuguese = creds.portuguese;
    let body = serde_json::to_vec(&serde_json::json!({
        "generation": hex::encode(generation),
        "total_bytes": total,
        "chunks": chunks,
        "sha": sha,
    }))
    .map_err(|_| UploadErr::Stop(invalid("backup: bad answer from the server")))?;
    let response = match send_raw(
        http,
        creds,
        reqwest::Method::PUT,
        "/v1/backup/blob",
        "blob",
        body,
        "application/json",
    )
    .await
    {
        Ok(response) => response,
        Err(_) => return Err(UploadErr::Again),
    };
    if response.status().as_u16() == 413 {
        return Err(UploadErr::Stop(account_full(portuguese)));
    }
    if !response.status().is_success() {
        return Err(UploadErr::Stop(refused(response, portuguese).await));
    }
    let bytes = response.bytes().await.map_err(|_| UploadErr::Again)?;
    let mut plan: ChunkPlan = serde_json::from_slice(&bytes)
        .map_err(|_| UploadErr::Stop(invalid("backup: bad answer from the server")))?;
    plan.missing.sort_unstable();
    plan.missing.dedup();
    Ok(plan)
}

async fn put_piece(
    http: &reqwest::Client,
    creds: &Creds,
    generation: &[u8; 32],
    sealed: &[u8],
    index: u32,
    chunks: u32,
) -> Result<(), UploadErr> {
    let portuguese = creds.portuguese;
    let body = frame(
        generation,
        index,
        chunks,
        sealed.len() as u64,
        piece_at(sealed, index),
    );
    for _ in 0..6 {
        let response = match send_raw(
            http,
            creds,
            reqwest::Method::PUT,
            "/v1/backup/blob",
            "blob",
            body.clone(),
            "application/octet-stream",
        )
        .await
        {
            Ok(response) => response,
            Err(_) => return Err(UploadErr::Again),
        };
        if response.status().is_success() {
            return Ok(());
        }
        if response.status().as_u16() == 429 {
            let wait = retry_after(&response);
            let _ = response.bytes().await;
            tokio::time::sleep(Duration::from_secs(wait)).await;
            continue;
        }
        if response.status().as_u16() == 413 {
            return Err(UploadErr::Stop(account_full(portuguese)));
        }
        return Err(UploadErr::Stop(refused(response, portuguese).await));
    }
    Err(UploadErr::Again)
}

async fn upload_sealed<F>(
    http: &reqwest::Client,
    creds: &Creds,
    generation: &[u8; 32],
    sealed: &[u8],
    mut progress: F,
) -> Result<(), CoreError>
where
    F: FnMut(u64),
{
    let portuguese = creds.portuguese;
    if sealed.is_empty() || shows_sqlite(sealed) {
        return Err(corrupted(portuguese));
    }
    let Some(chunks) = piece_count(sealed.len()) else {
        return Err(account_full(portuguese));
    };
    let sha = hex::encode(sha256(sealed));
    let mut stalls = 0u32;
    let mut announced = false;
    let mut previous: Option<Vec<u32>> = None;
    loop {
        let plan =
            match fetch_plan(http, creds, generation, sealed.len() as u64, chunks, &sha).await {
                Ok(plan) => plan,
                Err(UploadErr::Stop(error)) => return Err(error),
                Err(UploadErr::Again) => {
                    stalls += 1;
                    if stalls >= 3 {
                        return Err(stopped(portuguese, 1, chunks));
                    }
                    continue;
                }
            };
        if sealed.len() as u64 > plan.ceiling {
            return Err(account_full(portuguese));
        }
        if plan.missing.is_empty() {
            progress(sealed.len() as u64);
            return Ok(());
        }
        if previous.as_ref() == Some(&plan.missing) {
            stalls += 1;
        } else if previous
            .as_ref()
            .is_some_and(|before| plan.missing.len() < before.len())
        {
            stalls = 0;
        }
        if stalls >= 3 {
            return Err(stopped(
                portuguese,
                plan.missing[0].saturating_add(1),
                chunks,
            ));
        }
        previous = Some(plan.missing.clone());
        progress(confirmed_bytes(sealed.len(), &plan.missing));
        if !announced && plan.missing[0] > 0 {
            announced = true;
            let piece = plan.missing[0] + 1;
            eprintln!(
                "{}",
                say(
                    portuguese,
                    &format!(
                        "Backup: continuando do pedaço {piece} de {chunks}. Os pedaços anteriores ficam."
                    ),
                    &format!(
                        "Backup: continuing from piece {piece} of {chunks}. Earlier pieces stay."
                    ),
                )
            );
        }
        let mut sent_all = true;
        let mut failed_at = plan.missing[0];
        for index in &plan.missing {
            match put_piece(http, creds, generation, sealed, *index, chunks).await {
                Ok(()) => {
                    eprintln!(
                        "{}",
                        say(
                            portuguese,
                            &format!("Backup: pedaço {} de {chunks} confirmado.", index + 1),
                            &format!("Backup: piece {} of {chunks} confirmed.", index + 1),
                        )
                    );
                }
                Err(UploadErr::Again) => {
                    sent_all = false;
                    failed_at = *index;
                    stalls += 1;
                    break;
                }
                Err(UploadErr::Stop(error)) => return Err(error),
            }
        }
        if stalls >= 3 {
            return Err(stopped(portuguese, failed_at + 1, chunks));
        }
        if sent_all {
            stalls = 0;
        }
    }
}

impl RodaEngine {
    fn backup_creds(&self) -> Result<Creds, CoreError> {
        let e = self.lock();
        let acct = e.account().cloned().ok_or_else(|| invalid("no account"))?;
        let key = e
            .net
            .author
            .as_ref()
            .map(|a| a.key.clone())
            .ok_or_else(|| invalid("locked"))?;
        let relay_name = e.store.meta("relay_name")?.ok_or_else(|| {
            invalid(t(
                "Conecte ao servidor uma vez antes de ligar o backup.",
                "Connect to the server once before turning on backup.",
            ))
        })?;
        Ok(Creds {
            base: http_base(&acct.relay_url),
            relay_name,
            identity: acct.identity,
            key,
            portuguese: portuguese_of(self),
        })
    }

    fn save_status(&self, s: &BackupStatusDto) -> Result<(), CoreError> {
        let j = serde_json::to_string(s).map_err(|e| invalid(e.to_string()))?;
        self.lock().store.set_meta(STATUS_META, &j)?;
        Ok(())
    }

    async fn put_vault(
        &self,
        c: &Creds,
        mode: &str,
        keys: &Keys,
        kdf: serde_json::Value,
        generation: [u8; 32],
        vault: &Arc<dyn SecretVault>,
    ) -> Result<(), CoreError> {
        let k = match vault
            .load(VAULT_BACKUP.into())
            .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
        {
            Some(k) => k,
            None => backup::new_backup_key(),
        };
        let body = serde_json::json!({
            "generation": hex::encode(generation),
            "mode": mode,
            "verifier": hex::encode(keys.verifier()),
            "wrapped_key": hex::encode(backup::wrap_key(keys, &c.identity, &k)),
            "kdf": kdf,
        });
        signed(
            &http()?,
            c,
            reqwest::Method::PUT,
            "/v1/backup/vault",
            "vault",
            body.to_string().into_bytes(),
        )
        .await?;
        if !vault.save(VAULT_BACKUP.into(), k.to_vec())
            || !vault.save(VAULT_BACKUP_GENERATION.into(), generation.to_vec())
        {
            return Err(invalid(t(
                "Não deu para guardar a chave no Keychain.",
                "Couldn't store the key in the Keychain.",
            )));
        }
        Ok(())
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl RodaEngine {
    /// Turns on the encrypted backup protected by a password (8+ characters). The server
    /// counts wrong guesses and, after 10, the backup can never be opened again.
    pub async fn backup_turn_on_password(
        &self,
        password: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<BackupStatusDto, CoreError> {
        if password.chars().count() < backup::MIN_PASSWORD_CHARS {
            return Err(invalid(t(
                "A senha precisa ter pelo menos 8 caracteres.",
                "The password needs at least 8 characters.",
            )));
        }
        let c = self.backup_creds()?;
        let b = backup::blind(&password);
        let generation = backup::new_backup_key();
        let body = serde_json::json!({
            "blinded": hex::encode(b.blinded), "generation": hex::encode(generation),
        });
        let r = signed(
            &http()?,
            &c,
            reqwest::Method::POST,
            "/v1/backup/oprf",
            "oprf",
            body.to_string().into_bytes(),
        )
        .await?;
        let ev: Evaluated = json(r).await?;
        let kdf = Kdf::default();
        let password2 = password.clone();
        let identity = c.identity.clone();
        let evaluated = hex32(&ev.evaluated)?;
        let kdf2 = kdf.clone();
        // Argon2id with 64 MiB takes a moment: off the async threads.
        let keys = tokio::task::spawn_blocking(move || {
            backup::password_keys(&password2, &b, &evaluated, &identity, &kdf2)
        })
        .await
        .map_err(|e| invalid(e.to_string()))??;
        self.put_vault(
            &c,
            "passphrase",
            &keys,
            serde_json::to_value(&kdf).unwrap_or_default(),
            generation,
            &vault,
        )
        .await?;
        self.save_status(&BackupStatusDto {
            enabled: true,
            mode: "password".into(),
            ..Default::default()
        })?;
        self.backup_now(vault).await
    }

    /// Turns on the encrypted backup protected by a 64-digit recovery key, which this
    /// returns once: only the person keeps it.
    pub async fn backup_turn_on_recovery_key(
        &self,
        vault: Arc<dyn SecretVault>,
    ) -> Result<String, CoreError> {
        let c = self.backup_creds()?;
        let rk = backup::new_recovery_key();
        let keys = backup::recovery_keys(&rk)?;
        self.put_vault(
            &c,
            "recovery_key",
            &keys,
            serde_json::json!({"alg": "hkdf-sha256", "v": 1}),
            backup::new_backup_key(),
            &vault,
        )
        .await?;
        self.save_status(&BackupStatusDto {
            enabled: true,
            mode: "recovery_key".into(),
            ..Default::default()
        })?;
        self.backup_now(vault).await?;
        Ok(rk)
    }

    /// Seals this device's history and keys and sends them to the server in chunks.
    pub async fn backup_now(
        &self,
        vault: Arc<dyn SecretVault>,
    ) -> Result<BackupStatusDto, CoreError> {
        let portuguese = portuguese_of(self);
        let k: [u8; 32] = vault
            .load(VAULT_BACKUP.into())
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| invalid(t("O backup está desligado.", "Backup is off.")))?;
        let generation: [u8; 32] = vault
            .load(VAULT_BACKUP_GENERATION.into())
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| {
                invalid(t(
                    "Configure o backup novamente neste aparelho.",
                    "Set up backup again on this device.",
                ))
            })?;
        let c = self.backup_creds()?;
        let secret = |name: &str| -> Result<String, CoreError> {
            vault
                .load(name.into())
                .map(hex::encode)
                .ok_or_else(|| invalid("missing key"))
        };
        let prepared = {
            let e = self.lock();
            let acct = e.account().cloned().ok_or_else(|| invalid("no account"))?;
            let db = e.backup_snapshot()?;
            let snapshot = sha256(&db);
            let db_path = e.db_path.clone();
            Ok::<_, CoreError>(
                match decide(load_pending(&db_path), &generation, &snapshot) {
                    Decision::Resume(sealed) => Prepared::Resume { sealed, db_path },
                    Decision::Changed => Prepared::Changed { db_path },
                    Decision::Broken => Prepared::Broken { db_path },
                    Decision::Fresh => Prepared::Fresh {
                        header: Header {
                            version: 1,
                            identity: acct.identity,
                            relay_url: acct.relay_url,
                            identity_secret: secret(VAULT_IDENTITY)?,
                            agreement_secret: secret(VAULT_AGREEMENT)?,
                            created_ms: crate::engine::now_ms(),
                        },
                        db,
                        snapshot,
                        db_path,
                    },
                },
            )
        }?;
        let (sealed, db_path) = match prepared {
            Prepared::Changed { db_path } => {
                clear_pending(&db_path);
                return Err(history_changed(portuguese));
            }
            Prepared::Broken { db_path } => {
                clear_pending(&db_path);
                return Err(pending_unreadable(portuguese));
            }
            Prepared::Resume { sealed, db_path } => (sealed, db_path),
            Prepared::Fresh {
                header,
                db,
                snapshot,
                db_path,
            } => {
                let packed = compress(&db);
                let sealed = backup::seal_payload(&k, &header, &packed)?;
                if shows_sqlite(&sealed) {
                    return Err(invalid(say(
                        portuguese,
                        "O backup está corrompido.",
                        "The backup is corrupted.",
                    )));
                }
                save_pending(
                    &db_path,
                    &Pending {
                        generation,
                        snapshot,
                        sealed: sealed.clone(),
                    },
                )?;
                (sealed, db_path)
            }
        };
        let bytes = sealed.len() as u64;
        upload_sealed(&http()?, &c, &generation, &sealed, |_| {}).await?;
        clear_pending(&db_path);
        let mut s = self.backup_status();
        s.enabled = true;
        s.last_backup_ms = crate::engine::now_ms();
        s.bytes = bytes;
        self.save_status(&s)?;
        Ok(s)
    }

    /// Turns the backup off and deletes it from the server.
    pub async fn backup_turn_off(&self, vault: Arc<dyn SecretVault>) -> Result<(), CoreError> {
        let c = self.backup_creds()?;
        signed(
            &http()?,
            &c,
            reqwest::Method::DELETE,
            "/v1/backup",
            "delete",
            Vec::new(),
        )
        .await?;
        vault.delete(VAULT_BACKUP.into());
        vault.delete(VAULT_BACKUP_GENERATION.into());
        let db_path = self.lock().db_path.clone();
        self.lock().store.meta_delete(STATUS_META)?;
        clear_pending(&db_path);
        Ok(())
    }

    /// On a device without an account: finds @handle's backup on `relay_url`, opens it with
    /// the password or the recovery key, installs the history and makes this device a new
    /// device of that account.
    pub async fn restore_backup(
        &self,
        relay_url: String,
        handle: String,
        secret: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<AccountDto, CoreError> {
        let portuguese = portuguese_of(self);
        if self.lock().account().is_some() {
            return Err(invalid(t(
                "Este aparelho já tem uma conta.",
                "This device already has an account.",
            )));
        }
        let handle = roda_proto::normalize_handle(&handle)
            .ok_or_else(|| invalid(t("Esse @ não parece certo.", "That @ doesn't look right.")))?;
        let base = http_base(&relay_url);
        let http = http()?;
        let b = backup::blind(&secret);
        let r = http
            .post(format!("{base}/v1/backup/restore/start"))
            .header("content-type", "application/json")
            .body(
                serde_json::json!({"handle": handle, "blinded": hex::encode(b.blinded)})
                    .to_string(),
            )
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r, portuguese).await);
        }
        let s: Started = json(r).await?;
        let keys = match s.mode.as_str() {
            "recovery_key" => backup::recovery_keys(&secret)?,
            _ => {
                let kdf: Kdf = serde_json::from_value(s.kdf.clone())
                    .map_err(|_| invalid("unsupported backup format"))?;
                let evaluated = hex32(s.evaluated.as_deref().unwrap_or_default())?;
                let identity = s.identity.clone();
                tokio::task::spawn_blocking(move || {
                    backup::password_keys(&secret, &b, &evaluated, &identity, &kdf)
                })
                .await
                .map_err(|e| invalid(e.to_string()))??
            }
        };
        let auth = hex::encode(keys.auth);
        let r = http
            .post(format!("{base}/v1/backup/restore/open"))
            .header("content-type", "application/json")
            .body(
                serde_json::json!({
                    "handle": handle, "auth_key": auth, "generation": s.generation,
                })
                .to_string(),
            )
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r, portuguese).await);
        }
        let o: Opened = json(r).await?;
        if o.generation != s.generation {
            return Err(invalid("backup configuration changed; restart restore"));
        }
        let generation = o.generation.as_deref().map(hex32).transpose()?;
        let k = backup::unwrap_key(
            &keys,
            &s.identity,
            &hex::decode(&o.wrapped_key).map_err(|_| invalid("backup"))?,
        )?;
        let mut download_url =
            reqwest::Url::parse(&format!("{base}/v1/backup/restore/blob")).map_err(offline)?;
        download_url
            .query_pairs_mut()
            .append_pair("handle", &handle);
        if let Some(generation) = &s.generation {
            download_url
                .query_pairs_mut()
                .append_pair("generation", generation);
        }
        let r = http
            .get(download_url)
            .header("x-zoen-backup-auth", &auth)
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r, portuguese).await);
        }
        let sealed = r.bytes().await.map_err(offline)?;
        let (mut header, packed) = backup::open_payload(&k, &s.identity, &sealed)?;
        let db = decompress(&packed).map_err(|_| {
            invalid(say(
                portuguese,
                "O backup está corrompido.",
                "The backup is corrupted.",
            ))
        })?;
        // Restore onto the relay the person chose (the backup may predate a move).
        header.relay_url = relay_url.clone();
        let fail = || {
            invalid(t(
                "Não deu para guardar a chave no Keychain.",
                "Couldn't store the key in the Keychain.",
            ))
        };
        let root = hex::decode(&header.identity_secret).map_err(|_| invalid("backup"))?;
        let agreement = hex::decode(&header.agreement_secret).map_err(|_| invalid("backup"))?;
        let root_secret: [u8; 32] = root
            .as_slice()
            .try_into()
            .map_err(|_| invalid("backup identity"))?;
        let root_signer = roda_log::Signer::from_secret(&root_secret);
        if root_signer.id() != s.identity {
            return Err(invalid("backup identity"));
        }
        let device = roda_log::Signer::generate();
        let author = roda_log::Author::device(&root_signer, device.clone());
        let cert = author.cert.as_deref().expect("device certificate");
        let configuration = s
            .generation
            .as_deref()
            .ok_or_else(|| invalid("backup server does not support device recovery"))?;
        let signature = device.sign(&roda_proto::backup_enroll_message(
            &s.identity,
            &device.id(),
            cert,
            configuration,
        ));
        let r = http
            .post(format!("{base}/v1/backup/restore/enroll"))
            .header("content-type", "application/json")
            .body(
                serde_json::json!({
                    "handle": handle,
                    "auth_key": auth,
                    "generation": configuration,
                    "device": device.id(),
                    "cert": cert,
                    "sig": signature,
                })
                .to_string(),
            )
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r, portuguese).await);
        }
        {
            let mut e = self.lock();
            let device = e.restore_snapshot(&header, &db, device)?;
            if !vault.save(VAULT_IDENTITY.into(), root)
                || !vault.save(VAULT_DEVICE.into(), device.to_vec())
                || !vault.save(VAULT_AGREEMENT.into(), agreement.clone())
                || !vault.save(VAULT_BACKUP.into(), k.to_vec())
            {
                let _ = e.wipe();
                let _ = e.store.meta_delete("account");
                return Err(fail());
            }
            if let Some(generation) = generation {
                if !vault.save(VAULT_BACKUP_GENERATION.into(), generation.to_vec()) {
                    let _ = e.wipe();
                    let _ = e.store.meta_delete("account");
                    return Err(fail());
                }
            } else {
                vault.delete(VAULT_BACKUP_GENERATION.into());
            }
            e.unlock_profile(Some(agreement))?;
            let status = BackupStatusDto {
                enabled: true,
                mode: if s.mode == "recovery_key" {
                    "recovery_key".into()
                } else {
                    "password".into()
                },
                last_backup_ms: header.created_ms,
                bytes: sealed.len() as u64,
            };
            if let Ok(j) = serde_json::to_string(&status) {
                e.store.set_meta(STATUS_META, &j)?;
            }
        }
        self.account_dto()
            .ok_or_else(|| invalid("account after restore"))
    }
}

#[uniffi::export]
impl RodaEngine {
    /// Whether the backup is on, how, and when it last ran.
    pub fn backup_status(&self) -> BackupStatusDto {
        self.lock()
            .store
            .meta(STATUS_META)
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;

    fn header() -> Header {
        Header {
            version: 1,
            identity: "person".into(),
            relay_url: "http://relay.example".into(),
            identity_secret: "aa".into(),
            agreement_secret: "bb".into(),
            created_ms: 1,
        }
    }

    #[test]
    fn account_full_phrases_are_exact() {
        assert_eq!(
            account_full(true).to_string(),
            "Este backup não cabe na conta."
        );
        assert_eq!(
            account_full(false).to_string(),
            "This backup does not fit this account."
        );
    }

    #[test]
    fn compression_round_trips_and_happens_before_encryption() {
        for input in [
            Vec::new(),
            b"hi".to_vec(),
            b"abcabcabcabc".to_vec(),
            vec![b'a'; 10_000],
            {
                let mut mixed = vec![0u8; 1000];
                let mut state = 0x1234_5678u32;
                for byte in &mut mixed {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    *byte = (state >> 24) as u8;
                }
                mixed
            },
        ] {
            assert_eq!(decompress(&compress(&input)).unwrap(), input);
        }
        let mut db = b"SQLite format 3\0".to_vec();
        db.extend(std::iter::repeat_n(b'a', 8_000));
        let packed = compress(&db);
        assert!(packed.starts_with(COMPRESS_MAGIC));
        assert!(packed.len() < db.len());
        assert_eq!(decompress(&packed).unwrap(), db);
        let key = [7u8; 32];
        let sealed = backup::seal_payload(&key, &header(), &packed).unwrap();
        assert!(!shows_sqlite(&sealed));
        let (_, opened) = backup::open_payload(&key, "person", &sealed).unwrap();
        assert_eq!(opened, packed);
        assert_ne!(opened, db);
        assert_eq!(decompress(&opened).unwrap(), db);
    }

    #[test]
    fn interrupted_backup_reuses_the_same_ciphertext_or_asks_for_a_new_run() {
        let mut id = [0u8; 8];
        getrandom::getrandom(&mut id).unwrap();
        let dir = std::env::temp_dir().join(format!("zoen-u31-{}", hex::encode(id)));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("history.sqlite");
        let db_path = db_path.to_str().unwrap();
        let generation = [3u8; 32];
        let snapshot = sha256(b"history-v1");
        let sealed = b"sealed-ciphertext-v1".to_vec();
        assert!(matches!(
            decide(load_pending(db_path), &generation, &snapshot),
            Decision::Fresh
        ));
        save_pending(
            db_path,
            &Pending {
                generation,
                snapshot,
                sealed: sealed.clone(),
            },
        )
        .unwrap();
        match decide(load_pending(db_path), &generation, &snapshot) {
            Decision::Resume(got) => assert_eq!(got, sealed),
            other => panic!("expected the same ciphertext, got {other:?}"),
        }
        let changed = sha256(b"history-v2");
        assert!(matches!(
            decide(load_pending(db_path), &generation, &changed),
            Decision::Changed
        ));
        let pt = history_changed(true).to_string();
        let en = history_changed(false).to_string();
        assert!(pt.contains("Rode o backup de novo"));
        assert!(en.contains("Run the backup again"));
        assert!(!pt.contains("sealed-ciphertext"));
        let pending = pending_path(db_path).unwrap();
        std::fs::write(&pending, b"not a backup").unwrap();
        assert!(matches!(
            decide(load_pending(db_path), &generation, &snapshot),
            Decision::Broken
        ));
        assert!(pending_unreadable(true)
            .to_string()
            .contains("Rode o backup de novo"));
        assert!(pending_unreadable(false)
            .to_string()
            .contains("Run the backup again"));
        clear_pending(db_path);
        assert!(matches!(
            decide(load_pending(db_path), &generation, &snapshot),
            Decision::Fresh
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    struct Script {
        ceiling: u64,
        refuse_body: Option<Vec<u8>>,
        fail_plans: u32,
        drop_once: Option<u32>,
        plans: u32,
        dropped: bool,
        hits: Vec<u32>,
        stored: std::collections::BTreeSet<u32>,
    }

    fn content_length(head: &str) -> usize {
        head.lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().ok())
                    .flatten()
            })
            .unwrap_or(0)
    }

    fn read_request(socket: &mut std::net::TcpStream) -> Option<Vec<u8>> {
        let _ = socket.set_read_timeout(Some(Duration::from_secs(10)));
        let mut buf = Vec::new();
        let mut tmp = [0u8; 16 * 1024];
        let split = loop {
            let n = socket.read(&mut tmp).ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(index) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
            if buf.len() > 1024 * 1024 {
                return None;
            }
        };
        let len = content_length(&String::from_utf8_lossy(&buf[..split]));
        let mut body = buf[split..].to_vec();
        while body.len() < len {
            let n = socket.read(&mut tmp).ok()?;
            if n == 0 {
                return None;
            }
            body.extend_from_slice(&tmp[..n]);
        }
        body.truncate(len);
        Some(body)
    }

    fn write_response(socket: &mut std::net::TcpStream, status: &str, body: &[u8]) {
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = socket.write_all(head.as_bytes());
        let _ = socket.write_all(body);
    }

    fn handle(script: &mut Script, socket: &mut std::net::TcpStream, body: &[u8]) {
        if body.first() == Some(&b'{') {
            script.plans += 1;
            if script.plans <= script.fail_plans {
                let _ = socket.shutdown(std::net::Shutdown::Both);
                return;
            }
            if let Some(raw) = &script.refuse_body {
                write_response(socket, "413 Payload Too Large", raw);
                return;
            }
            let value: serde_json::Value = serde_json::from_slice(body).unwrap();
            let chunks = value["chunks"].as_u64().unwrap() as u32;
            let missing: Vec<u32> = (0..chunks)
                .filter(|index| !script.stored.contains(index))
                .collect();
            let raw = serde_json::to_vec(&serde_json::json!({
                "ceiling": script.ceiling,
                "missing": missing,
            }))
            .unwrap();
            write_response(socket, "200 OK", &raw);
            return;
        }
        assert_eq!(&body[..8], b"ZOENBC1\0");
        let index = u32::from_be_bytes(body[40..44].try_into().unwrap());
        script.hits.push(index);
        if script.drop_once == Some(index) && !script.dropped {
            script.dropped = true;
            let _ = socket.shutdown(std::net::Shutdown::Both);
            return;
        }
        script.stored.insert(index);
        write_response(socket, "201 Created", b"");
    }

    struct Running {
        base: String,
        script: Arc<std::sync::Mutex<Script>>,
        done: Arc<std::sync::atomic::AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for Running {
        fn drop(&mut self) {
            self.done.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn spawn_server(script: Script) -> Running {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let script = Arc::new(std::sync::Mutex::new(script));
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let script_thread = script.clone();
        let done_thread = done.clone();
        let thread = std::thread::spawn(move || {
            while !done_thread.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        let _ = socket.set_nonblocking(false);
                        if let Some(body) = read_request(&mut socket) {
                            handle(&mut script_thread.lock().unwrap(), &mut socket, &body);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Running {
            base: format!("http://127.0.0.1:{port}"),
            script,
            done,
            thread: Some(thread),
        }
    }

    async fn upload(
        server: &Running,
        sealed: &[u8],
        portuguese: bool,
        progress: impl FnMut(u64),
    ) -> Result<(), CoreError> {
        let creds = Creds {
            base: server.base.clone(),
            relay_name: "relay".into(),
            identity: "person".into(),
            key: roda_log::Signer::generate(),
            portuguese,
        };
        upload_sealed(&http()?, &creds, &[4u8; 32], sealed, progress).await
    }

    #[tokio::test]
    async fn resume_after_a_drop_does_not_resend_a_confirmed_piece() {
        let server = spawn_server(Script {
            ceiling: 1 << 30,
            refuse_body: None,
            fail_plans: 0,
            drop_once: Some(1),
            plans: 0,
            dropped: false,
            hits: Vec::new(),
            stored: std::collections::BTreeSet::new(),
        });
        let sealed = vec![0xA5; CHUNK_BYTES + 4];
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = seen.clone();
        upload(&server, &sealed, true, move |bytes| {
            record.lock().unwrap().push(bytes);
        })
        .await
        .unwrap();
        let script = server.script.lock().unwrap();
        assert_eq!(script.hits.iter().filter(|index| **index == 0).count(), 1);
        assert_eq!(script.hits, vec![0, 1, 1]);
        let progress = seen.lock().unwrap().clone();
        assert!(progress.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(*progress.last().unwrap(), sealed.len() as u64);
    }

    #[tokio::test]
    async fn a_too_small_account_does_not_echo_the_refusal_or_send_chunks() {
        for portuguese in [true, false] {
            let leaked = br#"{"ceiling":1,"leaked":"raw-413-body"}"#.to_vec();
            let server = spawn_server(Script {
                ceiling: 1,
                refuse_body: Some(leaked),
                fail_plans: 0,
                drop_once: None,
                plans: 0,
                dropped: false,
                hits: Vec::new(),
                stored: std::collections::BTreeSet::new(),
            });
            let error = upload(&server, &[9u8; 32], portuguese, |_| {})
                .await
                .unwrap_err();
            assert_eq!(error.to_string(), account_full(portuguese).to_string());
            assert!(!error.to_string().contains("raw-413-body"));
            assert!(!error.to_string().contains("leaked"));
            assert!(server.script.lock().unwrap().hits.is_empty());
        }
    }

    #[tokio::test]
    async fn a_declared_ceiling_below_the_sealed_backup_sends_no_chunk() {
        let server = spawn_server(Script {
            ceiling: 8,
            refuse_body: None,
            fail_plans: 0,
            drop_once: None,
            plans: 0,
            dropped: false,
            hits: Vec::new(),
            stored: std::collections::BTreeSet::new(),
        });
        let error = upload(&server, &[9u8; 64], true, |_| {}).await.unwrap_err();
        assert_eq!(error.to_string(), "Este backup não cabe na conta.");
        assert!(server.script.lock().unwrap().hits.is_empty());
    }

    #[tokio::test]
    async fn three_drops_say_which_piece_to_continue_from() {
        let server = spawn_server(Script {
            ceiling: 1 << 30,
            refuse_body: None,
            fail_plans: 3,
            drop_once: None,
            plans: 0,
            dropped: false,
            hits: Vec::new(),
            stored: std::collections::BTreeSet::new(),
        });
        let error = upload(&server, &[9u8; 32], true, |_| {}).await.unwrap_err();
        let text = error.to_string();
        assert_eq!(
            text,
            "O backup parou no pedaço 1 de 1. O que já foi confirmado continua. Rode o backup de novo para continuar desse pedaço."
        );
        assert!(!text.contains("413"));
        assert!(server.script.lock().unwrap().hits.is_empty());
        let server = spawn_server(Script {
            ceiling: 1 << 30,
            refuse_body: None,
            fail_plans: 3,
            drop_once: None,
            plans: 0,
            dropped: false,
            hits: Vec::new(),
            stored: std::collections::BTreeSet::new(),
        });
        let text = upload(&server, &[9u8; 32], false, |_| {})
            .await
            .unwrap_err()
            .to_string();
        assert_eq!(
            text,
            "The backup stopped at piece 1 of 1. What is already confirmed stays. Run the backup again to continue from that piece."
        );
    }
}
