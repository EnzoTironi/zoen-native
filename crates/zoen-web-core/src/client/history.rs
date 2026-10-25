//! Resumable ADR0045 transfer adapter. Only an authenticated signed outer already in
//! the canonical Space log can receive its opened inner from the linked device.

use super::*;
use device_link::{Bundle, HistoryBox};
use zeroize::Zeroizing;

const MAX_MANIFEST: usize = 64 << 10;
const MAX_CHUNK: usize = (1 << 20) + 16;
const MAX_HISTORY: usize = 64 << 20;
const MAX_EVENTS: usize = 100_000;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    transfer: Option<Transfer>,
    complete: Option<Complete>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Transfer {
    manifest: HistoryBox,
    chunks: BTreeMap<u32, Vec<u8>>,
    resumed: u32,
}

#[derive(Clone, Serialize, Deserialize)]
struct Complete {
    transfer: String,
    messages: u32,
    chunks: u32,
    resumed: u32,
    cleaned: bool,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub update: Update,
    pub work: HistoryWork,
}

#[derive(Serialize)]
pub struct HistoryCleanup {
    pub transfer: String,
    pub device: String,
    pub timestamp: String,
    pub signature: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryWork {
    FetchManifest {
        box_id: String,
    },
    Download {
        transfer: String,
        index: u32,
        sha256: String,
    },
    WaitingSync,
    Complete {
        messages: u32,
        chunks: u32,
        resumed: u32,
        cleanup: Option<HistoryCleanup>,
    },
}

fn hex32(value: &str) -> Result<[u8; 32], String> {
    hex::decode(value)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| "Invalid linked-device history key.".into())
}

fn valid_manifest(manifest: &HistoryBox) -> bool {
    let hash =
        |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    hash(&manifest.transfer)
        && hash(&manifest.key)
        && !manifest.chunks.is_empty()
        && manifest.chunks.len() <= 900
        && manifest.chunks.iter().all(|sha| hash(sha))
}

impl<S: Signer> Client<S> {
    pub(super) fn history_hint(&self, from: &str, to: &str, sealed: &[u8]) -> bool {
        if from != self.account.primary
            || to != self.account.device
            || self.saved.history.complete.is_some()
            || self.account.link_secret.is_empty()
            || sealed.len() > MAX_MANIFEST
        {
            return false;
        }
        let peer = device_link::Peer {
            device: from.into(),
            their_hpke: self.account.primary_hpke.clone(),
            my_hpke_secret: self.account.hpke_secret.clone(),
            secret: self.account.link_secret.clone(),
        };
        matches!(
            peer.open(sealed),
            Some(device_link::DeviceMsg::HistoryReady)
        )
    }

    pub fn history_status(&self) -> String {
        let transfer = self.saved.history.transfer.as_ref();
        let complete = self.saved.history.complete.as_ref();
        serde_json::json!({
            "initial_pending": !self.account.link_secret.is_empty() && complete.is_none(),
            "manifest_received": transfer.is_some() || complete.is_some(),
            "chunks_received": transfer.map(|transfer| transfer.chunks.len() as u32).or_else(|| complete.map(|done| done.chunks)).unwrap_or(0),
            "chunks_total": transfer.map(|transfer| transfer.manifest.chunks.len() as u32).or_else(|| complete.map(|done| done.chunks)).unwrap_or(0),
            // The host owns the asynchronous network pump, not this synchronous core.
            "receive_active": false
        }).to_string()
    }

    /// Start/retry one initial handover. The host persists every returned Update
    /// before requesting its work, including the one-time manifest mailbox.
    pub fn history_start(&mut self, now: i64) -> Result<HistoryResponse, String> {
        if let Some(transfer) = self.saved.history.transfer.as_mut() {
            transfer.resumed = transfer.chunks.len() as u32;
        }
        self.history_response(now)
    }

    pub fn history_manifest(&mut self, sealed: &[u8], now: i64) -> Result<HistoryResponse, String> {
        if sealed.len() < 48 || sealed.len() > MAX_MANIFEST {
            return Err("Invalid linked-device history manifest size.".into());
        }
        let secret = Zeroizing::new(hex32(&self.account.link_secret)?);
        let private = Zeroizing::new(
            hex::decode(&self.account.hpke_secret).map_err(|_| "Invalid linked-device key.")?,
        );
        let plain = Zeroizing::new(
            device_link::open(&private, &secret, "history", sealed)
                .ok_or("The linked device's history could not be opened.")?,
        );
        let manifest: HistoryBox = serde_json::from_slice(&plain)
            .map_err(|_| "Invalid linked-device history manifest.")?;
        if !valid_manifest(&manifest) {
            return Err("Invalid linked-device history manifest.".into());
        }
        if let Some(existing) = &self.saved.history.transfer {
            if serde_json::to_vec(&existing.manifest).unwrap()
                != serde_json::to_vec(&manifest).unwrap()
            {
                return Err("A different history transfer is already being restored.".into());
            }
        } else if let Some(complete) = &self.saved.history.complete {
            if complete.transfer != manifest.transfer {
                return Err("This browser has already restored its initial history.".into());
            }
        } else {
            self.saved.history.transfer = Some(Transfer {
                manifest,
                chunks: BTreeMap::new(),
                resumed: 0,
            });
        }
        self.history_response(now)
    }

    pub fn history_chunk(
        &mut self,
        index: u32,
        sealed: &[u8],
        now: i64,
    ) -> Result<HistoryResponse, String> {
        let transfer = self
            .saved
            .history
            .transfer
            .as_ref()
            .ok_or("No linked-device history transfer is pending.")?;
        let expected = transfer
            .manifest
            .chunks
            .get(index as usize)
            .ok_or("Invalid history chunk index.")?;
        if sealed.len() < 16
            || sealed.len() > MAX_CHUNK
            || event_log::sha256_hex(sealed) != *expected
        {
            return Err("A history chunk does not match its authenticated manifest.".into());
        }
        if let Some(current) = transfer.chunks.get(&index) {
            if current != sealed {
                return Err("A history chunk conflicts with the retained transfer.".into());
            }
            return self.history_response(now);
        }
        let key = Zeroizing::new(hex32(&transfer.manifest.key)?);
        let _plain = Zeroizing::new(
            device_link::open_chunk(&key, index, sealed)
                .ok_or("A history chunk could not be opened.")?,
        );
        let retained = transfer
            .chunks
            .values()
            .map(|chunk| chunk.len().saturating_sub(16))
            .sum::<usize>();
        if retained
            .checked_add(sealed.len() - 16)
            .is_none_or(|size| size > MAX_HISTORY)
        {
            return Err("This history transfer is too large for the browser.".into());
        }
        self.saved
            .history
            .transfer
            .as_mut()
            .unwrap()
            .chunks
            .insert(index, sealed.to_vec());
        self.history_response(now)
    }

    /// Call again after SyncDone. Missing ciphertext is never replaced by invented
    /// standalone events or an unverified second history chain.
    pub fn history_finish(&mut self, now: i64) -> Result<HistoryResponse, String> {
        self.history_response(now)
    }

    /// Only after the host durably saved the imported Update and deleted the remote
    /// transfer. A failed DELETE remains resumable through Complete.cleanup.
    pub fn history_cleaned(&mut self, now: i64) -> Result<HistoryResponse, String> {
        let complete = self
            .saved
            .history
            .complete
            .as_mut()
            .ok_or("History has not been restored yet.")?;
        complete.cleaned = true;
        self.history_response(now)
    }

    fn history_response(&mut self, now: i64) -> Result<HistoryResponse, String> {
        self.view_now_ms = now;
        if self
            .saved
            .history
            .transfer
            .as_ref()
            .is_some_and(|transfer| transfer.chunks.len() == transfer.manifest.chunks.len())
            && self.synced
        {
            self.import_history_transfer()?;
        }
        let work = if let Some(complete) = &self.saved.history.complete {
            let cleanup = if complete.cleaned {
                None
            } else {
                let payload = protocol::transfer_message("delete", &complete.transfer, "", "", now);
                let signature = self
                    .signer
                    .sign(&payload)
                    .map_err(|_| "Could not authorize history cleanup.")?;
                Some(HistoryCleanup {
                    transfer: complete.transfer.clone(),
                    device: self.account.device.clone(),
                    timestamp: now.to_string(),
                    signature: hex::encode(signature),
                })
            };
            HistoryWork::Complete {
                messages: complete.messages,
                chunks: complete.chunks,
                resumed: complete.resumed,
                cleanup,
            }
        } else if let Some(transfer) = &self.saved.history.transfer {
            if let Some((index, sha)) = transfer
                .manifest
                .chunks
                .iter()
                .enumerate()
                .find(|(index, _)| !transfer.chunks.contains_key(&(*index as u32)))
            {
                HistoryWork::Download {
                    transfer: transfer.manifest.transfer.clone(),
                    index: index as u32,
                    sha256: sha.clone(),
                }
            } else {
                HistoryWork::WaitingSync
            }
        } else {
            event_log::parse_device_key(&self.account.primary)
                .ok_or("This browser has no linked primary device.")?;
            let secret = Zeroizing::new(hex32(&self.account.link_secret)?);
            HistoryWork::FetchManifest {
                box_id: device_link::box_id(&secret, "history"),
            }
        };
        Ok(HistoryResponse {
            update: self.update(Vec::new())?,
            work,
        })
    }

    fn import_history_transfer(&mut self) -> Result<(), String> {
        let transfer = self
            .saved
            .history
            .transfer
            .as_ref()
            .ok_or("No history transfer is pending.")?;
        let key = Zeroizing::new(hex32(&transfer.manifest.key)?);
        let mut plain = Zeroizing::new(Vec::new());
        for index in 0..transfer.manifest.chunks.len() {
            let sealed = transfer
                .chunks
                .get(&(index as u32))
                .ok_or("A history chunk is missing.")?;
            // Recheck persisted material too; storage never acts as a crypto oracle.
            if sealed.len() > MAX_CHUNK
                || event_log::sha256_hex(sealed) != transfer.manifest.chunks[index]
            {
                return Err("A retained history chunk does not match.".into());
            }
            let opened = Zeroizing::new(
                device_link::open_chunk(&key, index as u32, sealed)
                    .ok_or("A retained history chunk could not be opened.")?,
            );
            if plain
                .len()
                .checked_add(opened.len())
                .is_none_or(|length| length > MAX_HISTORY)
            {
                return Err("History is too large for the browser.".into());
            }
            plain.extend_from_slice(&opened);
        }
        let bundle: Bundle = serde_json::from_slice(&plain)
            .map_err(|_| "The linked-device history is not a valid bundle.")?;
        if bundle.events.len() > MAX_EVENTS {
            return Err("This history bundle contains too many events.".into());
        }
        let messages = self.import_history_events(bundle.events)?;
        let transfer = self.saved.history.transfer.take().unwrap();
        self.saved.history.complete = Some(Complete {
            transfer: transfer.manifest.transfer,
            messages,
            chunks: transfer.manifest.chunks.len() as u32,
            resumed: transfer.resumed,
            cleaned: false,
        });
        Ok(())
    }

    pub(super) fn import_history_events(&mut self, events: Vec<Event>) -> Result<u32, String> {
        let mut changed: BTreeMap<String, Vec<Event>> = BTreeMap::new();
        let mut positions = BTreeSet::new();
        let mut messages = 0u32;
        for event in events {
            if !positions.insert((event.space.clone(), event.seq)) {
                return Err("The linked-device history repeats a relay position.".into());
            }
            let Some(local) = self.logs.get(&event.space).and_then(|log| {
                usize::try_from(event.seq)
                    .ok()
                    .and_then(|index| log.events().get(index))
            }) else {
                continue;
            };
            // An already opened record is idempotent; a clear event cannot become a
            // different signed message through the history side channel.
            if !matches!(local.body, EventBody::Sealed { .. }) {
                continue;
            }
            let opened = device_link::opened_history_event(local, event.clone())
                .ok_or("A history entry does not match the signed relay record.")?;
            let entries = changed
                .entry(event.space.clone())
                .or_insert_with(|| self.logs[&event.space].events().to_vec());
            entries[event.seq as usize] = opened;
            messages = messages.checked_add(1).ok_or("Too many history entries.")?;
        }
        let verified = changed
            .into_iter()
            .map(|(space, events)| {
                SpaceLog::from_events(&space, events)
                    .map(|log| (space, log))
                    .map_err(|_| {
                        "The opened history does not verify against the relay chain.".to_string()
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (space, log) in verified {
            self.logs.insert(space, log);
        }
        Ok(messages)
    }
}

#[cfg(test)]
mod tests;
