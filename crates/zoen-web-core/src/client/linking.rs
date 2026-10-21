//! Device handover stays in the canonical encrypted client journal. The worker's
//! root authority only seals the identity box; it never enters this journal.
use super::*;
use crate::Primary;
use device_link::{DeviceMsg, Peer};
use serde_json::{json, Value};

const LINK_REQUEST_BASE: u64 = 0x9000_0000;
const DEVICE_REQUEST: u64 = 0x8000_0041;
const PAGE_LIMIT: usize = 100;
const MAX_DEVICE_MESSAGE: usize = 1 << 20;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    handover: Option<Handover>,
    request_sequence: u64,
    peers: BTreeMap<String, Peer>,
    certified: BTreeSet<String>,
    joins: BTreeMap<String, BTreeSet<String>>,
    sending: Option<Sending>,
    pages: BTreeMap<String, PageRequest>,
    complete: BTreeSet<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Handover {
    request_id: u64,
    device: String,
    code: String,
    check: String,
    box_id: String,
    sealed: Vec<u8>,
    cert: String,
    delivered: bool,
    failure: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct PageRequest {
    req: String,
    before: u64,
    answer: Option<(u32, bool)>,
    failure: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Sending {
    request_id: u64,
    device: String,
    transfer: String,
    chunks: Vec<Vec<u8>>,
    uploaded: usize,
    sealed_manifest: Vec<u8>,
    delivered: bool,
    messages: u32,
    hint_sent: bool,
    failure: Option<String>,
}
#[derive(Default)]
pub(super) struct Network {
    due: i64,
    page_due: BTreeMap<String, i64>,
    history_due: i64,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    LinkCodeCheck {
        code: String,
    },
    HistorySend {
        device_id: String,
    },
    HistorySendResume {},
    HistoryFailed {
        transfer: String,
        message: String,
    },
    HistoryUploaded {
        transfer: String,
        index: usize,
        sha256: String,
    },
    LinkStatus {
        device_id: String,
    },
    LinkProgress {
        device_id: String,
    },
    LoadOlder {
        space_id: String,
    },
    OlderStatus {
        space_id: String,
        request_id: String,
    },
}

impl<S: Signer> Client<S> {
    /// One durable namespace for retained profile/link/history protocol requests.
    /// Keep the existing journal sequence so in-flight saved links never collide
    /// with a new profile lookup across reconnect or process reopen.
    pub(super) fn next_retained_request_id(&mut self) -> Result<u64, String> {
        let sequence = self
            .saved
            .linking
            .request_sequence
            .checked_add(1)
            .ok_or("Request identifier limit reached.")?;
        let id = LINK_REQUEST_BASE
            .checked_add(sequence)
            .filter(|id| *id <= u64::from(u32::MAX))
            .ok_or("Request identifier limit reached.")?;
        self.saved.linking.request_sequence = sequence;
        Ok(id)
    }

    pub fn link_prepare(
        &mut self,
        primary: &Primary,
        code: &str,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        let parsed = device_link::LinkCode::parse(code).ok_or("Código inválido.")?;
        if !self.synced {
            return Err("Aguarde a conexão terminar de sincronizar.".into());
        }
        if let Some(current) = &mut self.saved.linking.handover {
            if current.code == parsed.encode() {
                current.failure = None;
                self.linking_net.due = 0;
                let result = json!({"device_id":current.device,"check":current.check});
                return self.link_result(result, now);
            }
            if !current.delivered && current.failure.is_none() {
                return Err("Termine o vínculo que já está em andamento.".into());
            }
        }
        let mut account = self.account.clone();
        let (key, version, seen, fields) = self.profile_link_fields()?;
        account.profile_key = key;
        account.profile_key_version = version;
        account.profile_seen_version = seen;
        account.profile_fields = fields;
        let sealed = primary.seal_link(&parsed, &account)?;
        let device = sealed.device.clone();
        let request_id = self.next_retained_request_id()?;
        self.saved.linking.peers.insert(device.clone(), sealed.peer);
        self.saved.linking.handover = Some(Handover {
            request_id,
            device: device.clone(),
            code: parsed.encode(),
            check: parsed.check_digits(),
            box_id: sealed.box_id,
            sealed: sealed.sealed,
            cert: sealed.cert,
            delivered: false,
            failure: None,
        });
        self.linking_net.due = 0;
        self.link_result(
            json!({"device_id":device,"check":parsed.check_digits()}),
            now,
        )
    }

    fn link_result(&mut self, result: Value, now: i64) -> Result<DocumentResponse, String> {
        self.view_now_ms = now;
        Ok(DocumentResponse {
            result,
            update: Some(self.update(Vec::new())?),
            downloads: Vec::new(),
            uploads: Vec::new(),
        })
    }

    pub fn link_request(&mut self, request: &str, now: i64) -> Result<DocumentResponse, String> {
        if request.len() > 8192 {
            return Err("Pedido de vínculo inválido.".into());
        }
        self.view_now_ms = now;
        let command =
            serde_json::from_str::<Command>(request).map_err(|_| "Pedido de vínculo inválido.")?;
        let read_only = matches!(
            &command,
            Command::HistorySendResume {}
                | Command::LinkCodeCheck { .. }
                | Command::LinkStatus { .. }
                | Command::LinkProgress { .. }
                | Command::OlderStatus { .. }
        );
        let result = match command {
            Command::LinkCodeCheck { code } => json!(device_link::LinkCode::parse(&code)
                .ok_or("Código inválido.")?
                .check_digits()),
            Command::HistorySend { device_id } => {
                self.prepare_outgoing_history(&device_id)?;
                self.outgoing_history_work(now)?
            }
            Command::HistorySendResume {} => self.outgoing_history_work(now)?,
            Command::HistoryFailed { transfer, message } => {
                if message.len() > 512 {
                    return Err("Falha de transferência inválida.".into());
                }
                let sending = self
                    .saved
                    .linking
                    .sending
                    .as_mut()
                    .filter(|sending| sending.transfer == transfer && !sending.delivered)
                    .ok_or("Transferência indisponível.")?;
                sending.failure = Some(message);
                json!({"kind":"failed"})
            }
            Command::HistoryUploaded {
                transfer,
                index,
                sha256,
            } => {
                let sending = self
                    .saved
                    .linking
                    .sending
                    .as_mut()
                    .filter(|sending| sending.transfer == transfer && !sending.delivered)
                    .ok_or("Transferência indisponível.")?;
                let chunk = sending
                    .chunks
                    .get(index)
                    .ok_or("Parte inválida do histórico.")?;
                if index != sending.uploaded || event_log::sha256_hex(chunk) != sha256 {
                    return Err("A parte enviada não corresponde ao histórico.".into());
                }
                sending.uploaded += 1;
                self.outgoing_history_work(now)?
            }
            Command::LinkStatus { device_id } => {
                let link = self
                    .saved
                    .linking
                    .handover
                    .as_ref()
                    .filter(|link| link.device == device_id)
                    .ok_or("Este aparelho não foi vinculado aqui.")?;
                json!({"delivered":link.delivered,"failure":link.failure})
            }
            Command::LinkProgress { device_id } => {
                if !self.saved.linking.peers.contains_key(&device_id) {
                    return Err("Este aparelho não foi vinculado aqui.".into());
                }
                let mut done = 0;
                let mut total = 0;
                for space in self.logs.keys().filter(|space| {
                    self.metadata(space)
                        .is_some_and(|(_, _, p)| p == Privacy::EndToEnd)
                }) {
                    if let Ok(Some(leaves)) = self.device()?.committed_leaves(space) {
                        total += 1;
                        if leaves.contains_key(&(self.account.identity.clone(), device_id.clone()))
                        {
                            done += 1;
                        }
                    }
                }
                json!([done, total])
            }
            Command::LoadOlder { space_id } => {
                if !self.roster(&space_id).contains_key(&self.account.identity) {
                    return Err("Conversa indisponível.".into());
                }
                if self.account.link_secret.is_empty()
                    || self.saved.linking.complete.contains(&space_id)
                {
                    json!({"request_id":null,"loaded":0,"more":false})
                } else {
                    let before = self
                        .logs
                        .get(&space_id)
                        .and_then(|log| {
                            log.events()
                                .iter()
                                .find(|event| event.sealed_wire.is_some())
                                .map(|event| event.seq)
                                .or(Some(log.next_seq()))
                        })
                        .unwrap_or(0);
                    // A retry after process interruption reuses exactly the pending boundary.
                    let pending = self
                        .saved
                        .linking
                        .pages
                        .entry(space_id.clone())
                        .or_insert_with(|| PageRequest {
                            req: browser_id("pg", now),
                            before,
                            answer: None,
                            failure: None,
                        });
                    if pending.answer.is_some() || pending.failure.is_some() {
                        *pending = PageRequest {
                            req: browser_id("pg", now),
                            before,
                            answer: None,
                            failure: None,
                        };
                    }
                    json!({"request_id":pending.req,"loaded":0,"more":true})
                }
            }
            Command::OlderStatus {
                space_id,
                request_id,
            } => {
                let pending = self
                    .saved
                    .linking
                    .pages
                    .get(&space_id)
                    .filter(|pending| pending.req == request_id)
                    .ok_or("Esta página não está mais pendente.")?;
                if let Some(failure) = &pending.failure {
                    return Ok(DocumentResponse {
                        result: json!({"pending":false,"loaded":0,"more":true,"failure":failure}),
                        update: None,
                        downloads: Vec::new(),
                        uploads: Vec::new(),
                    });
                }
                match pending.answer {
                    Some((loaded, more)) => json!({"pending":false,"loaded":loaded,"more":more}),
                    None => json!({"pending":true,"loaded":0,"more":true}),
                }
            }
        };
        if read_only {
            return Ok(DocumentResponse {
                result,
                update: None,
                downloads: Vec::new(),
                uploads: Vec::new(),
            });
        }
        self.link_result(result, now)
    }

    pub(super) fn linking_reply(
        &mut self,
        id: u64,
        result: &Result<Reply, String>,
    ) -> Result<bool, String> {
        if let Some(link) = self
            .saved
            .linking
            .handover
            .as_mut()
            .filter(|link| link.request_id == id)
        {
            match result {
                Ok(Reply::Done) => {
                    link.delivered = true;
                    link.sealed.clear();
                    self.saved.linking.certified.insert(link.device.clone());
                }
                Err(error) => link.failure = Some(error.clone()),
                _ => link.failure = Some("O servidor não confirmou o vínculo.".into()),
            }
            return Ok(true);
        }
        if let Some(sending) = self
            .saved
            .linking
            .sending
            .as_mut()
            .filter(|sending| sending.request_id == id)
        {
            match result {
                Ok(Reply::Done) => {
                    sending.delivered = true;
                    sending.chunks.clear();
                    sending.sealed_manifest.clear();
                }
                Err(error) => sending.failure = Some(error.clone()),
                _ => sending.failure = Some("O servidor não confirmou o histórico.".into()),
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn prepare_outgoing_history(&mut self, device: &str) -> Result<(), String> {
        if self.saved.membership.revoked.contains(device) {
            return Err("Este aparelho foi removido da conta.".into());
        }
        if let Some(sending) = &mut self.saved.linking.sending {
            if sending.device == device {
                sending.failure = None;
                self.linking_net.history_due = 0;
                return Ok(());
            }
            if !sending.delivered {
                return Err("Termine o envio de histórico em andamento.".into());
            }
        }
        if !self.saved.linking.certified.contains(device)
            || self.saved.membership.revoked.contains(device)
        {
            return Err("Este aparelho não foi vinculado aqui.".into());
        }
        let peer = self
            .saved
            .linking
            .peers
            .get(device)
            .ok_or("A chave do vínculo não está disponível.")?;
        let mut events = Vec::new();
        for (space, log) in &self.logs {
            if !self.roster(space).contains_key(&self.account.identity) {
                continue;
            }
            let opened = log
                .events()
                .iter()
                .filter(|event| event.sealed_wire.is_some())
                .collect::<Vec<_>>();
            events.extend(
                opened[opened.len().saturating_sub(500)..]
                    .iter()
                    .map(|event| (*event).clone()),
            );
        }
        if events.len() > 100_000 {
            return Err("Histórico grande demais para este navegador.".into());
        }
        let messages = events.len() as u32;
        let plain = zeroize::Zeroizing::new(
            serde_json::to_vec(&device_link::Bundle { events })
                .map_err(|_| "Histórico inválido.")?,
        );
        if plain.len() > 64 << 20 {
            return Err("Histórico grande demais para este navegador.".into());
        }
        let key = zeroize::Zeroizing::new(device_link::random::<32>());
        let transfer = hex::encode(device_link::random::<32>());
        let chunks = plain
            .chunks(1 << 20)
            .enumerate()
            .map(|(index, part)| device_link::seal_chunk(&key, index as u32, part))
            .collect::<Vec<_>>();
        let manifest = device_link::HistoryBox {
            transfer: transfer.clone(),
            key: hex::encode(key.as_ref()),
            chunks: chunks
                .iter()
                .map(|chunk| event_log::sha256_hex(chunk))
                .collect(),
        };
        let manifest = zeroize::Zeroizing::new(
            serde_json::to_vec(&manifest).map_err(|_| "Histórico inválido.")?,
        );
        let secret = zeroize::Zeroizing::new(peer.secret().ok_or("Chave de vínculo inválida.")?);
        let sealed_manifest = device_link::seal(
            &hex::decode(&peer.their_hpke).map_err(|_| "Aparelho inválido.")?,
            &secret,
            "history",
            &manifest,
        )
        .ok_or("Não foi possível proteger o histórico.")?;
        let request_id = self.next_retained_request_id()?;
        self.saved.linking.sending = Some(Sending {
            request_id,
            device: device.into(),
            transfer,
            chunks,
            uploaded: 0,
            sealed_manifest,
            delivered: false,
            messages,
            hint_sent: false,
            failure: None,
        });
        Ok(())
    }

    fn outgoing_history_work(&self, now: i64) -> Result<Value, String> {
        let Some(sending) = &self.saved.linking.sending else {
            return Ok(json!({"kind":"idle"}));
        };
        if self.saved.membership.revoked.contains(&sending.device) {
            return Err("Este aparelho foi removido da conta.".into());
        }
        if let Some(error) = &sending.failure {
            return Err(error.clone());
        }
        if sending.delivered {
            return Ok(json!({"kind":"complete","messages":sending.messages}));
        }
        if let Some(chunk) = sending.chunks.get(sending.uploaded) {
            let index = sending.uploaded;
            let sha = event_log::sha256_hex(chunk);
            let payload =
                protocol::transfer_message("put", &sending.transfer, &index.to_string(), &sha, now);
            let signature = self
                .signer
                .sign(&payload)
                .map_err(|_| "Não foi possível autorizar o histórico.")?;
            return Ok(
                json!({"kind":"upload","transfer":sending.transfer,"index":index,"sha256":sha,"bytes":chunk,"device":self.account.device,"timestamp":now.to_string(),"signature":hex::encode(signature)}),
            );
        }
        Ok(json!({"kind":"waiting"}))
    }

    pub(super) fn linking_frames(&mut self, frames: &mut Vec<Vec<u8>>) -> Result<(), String> {
        if !self.synced {
            return Ok(());
        }
        if let Some(link) = &self.saved.linking.handover {
            if !link.delivered && link.failure.is_none() && self.linking_net.due <= self.view_now_ms
            {
                frames.push(
                    ClientFrame::Req {
                        id: link.request_id,
                        op: Op::DeliverLink {
                            id: link.box_id.clone(),
                            sealed: link.sealed.clone(),
                            device: Some(protocol::DeviceCertificate {
                                device: link.device.clone(),
                                cert: link.cert.clone(),
                            }),
                        },
                    }
                    .encode(),
                );
                self.linking_net.due = self.view_now_ms.saturating_add(5000);
            }
        }
        if let Some(sending) = self.saved.linking.sending.as_ref().filter(|sending| {
            !sending.delivered
                && sending.failure.is_none()
                && sending.uploaded == sending.chunks.len()
                && !self.saved.membership.revoked.contains(&sending.device)
                && self.linking_net.history_due <= self.view_now_ms
        }) {
            let secret = self
                .saved
                .linking
                .peers
                .get(&sending.device)
                .and_then(Peer::secret)
                .ok_or("Chave de vínculo indisponível.")?;
            frames.push(
                ClientFrame::Req {
                    id: sending.request_id,
                    op: Op::DeliverLink {
                        id: device_link::box_id(&secret, "history"),
                        sealed: sending.sealed_manifest.clone(),
                        device: None,
                    },
                }
                .encode(),
            );
            self.linking_net.history_due = self.view_now_ms.saturating_add(5000);
        }
        if let Some(sending) = self.saved.linking.sending.as_mut().filter(|sending| {
            sending.delivered
                && !sending.hint_sent
                && !self.saved.membership.revoked.contains(&sending.device)
        }) {
            if let Some(sealed) = self
                .saved
                .linking
                .peers
                .get(&sending.device)
                .and_then(|peer| peer.seal(&DeviceMsg::HistoryReady))
            {
                frames.push(
                    ClientFrame::Req {
                        id: DEVICE_REQUEST,
                        op: Op::SendDevice {
                            to: sending.device.clone(),
                            sealed,
                        },
                    }
                    .encode(),
                );
                sending.hint_sent = true;
            }
        }
        let mut joins = Vec::new();
        for device in self
            .saved
            .linking
            .certified
            .iter()
            .filter(|device| !self.saved.membership.revoked.contains(*device))
        {
            for space in self.logs.keys().filter(|space| {
                self.metadata(space)
                    .is_some_and(|(_, _, p)| p == Privacy::EndToEnd)
                    && self.roster(space).contains_key(&self.account.identity)
                    && self
                        .saved
                        .linking
                        .joins
                        .get(device)
                        .is_none_or(|joined| !joined.contains(*space))
            }) {
                joins.push((space.clone(), device.clone()));
            }
        }
        for (space, device) in joins {
            self.enqueue_control(
                &space,
                EventBody::DeviceJoining {
                    device: device.clone(),
                },
                self.view_now_ms,
            )?;
            self.saved
                .linking
                .joins
                .entry(device)
                .or_default()
                .insert(space);
        }
        for (space, pending) in &self.saved.linking.pages {
            if self
                .saved
                .membership
                .revoked
                .contains(&self.account.primary)
                || pending.answer.is_some()
                || pending.failure.is_some()
                || self
                    .linking_net
                    .page_due
                    .get(space)
                    .is_some_and(|due| *due > self.view_now_ms)
            {
                continue;
            }
            let peer = Peer {
                device: self.account.primary.clone(),
                their_hpke: self.account.primary_hpke.clone(),
                my_hpke_secret: self.account.hpke_secret.clone(),
                secret: self.account.link_secret.clone(),
            };
            if let Some(sealed) = peer.seal(&DeviceMsg::PageRequest {
                req: pending.req.clone(),
                space: space.clone(),
                before: pending.before,
                limit: PAGE_LIMIT as u32,
            }) {
                frames.push(
                    ClientFrame::Req {
                        id: DEVICE_REQUEST,
                        op: Op::SendDevice {
                            to: peer.device,
                            sealed,
                        },
                    }
                    .encode(),
                );
                self.linking_net
                    .page_due
                    .insert(space.clone(), self.view_now_ms.saturating_add(5000));
            }
        }
        Ok(())
    }

    pub(super) fn linking_deadline(&self) -> Option<i64> {
        let link = self
            .saved
            .linking
            .handover
            .as_ref()
            .filter(|link| !link.delivered && link.failure.is_none())
            .map(|_| self.linking_net.due);
        let history = self
            .saved
            .linking
            .sending
            .as_ref()
            .filter(|sending| {
                !sending.delivered
                    && sending.failure.is_none()
                    && !self.saved.membership.revoked.contains(&sending.device)
            })
            .map(|sending| {
                if sending.uploaded < sending.chunks.len() {
                    self.view_now_ms.saturating_add(5000)
                } else {
                    self.linking_net.history_due
                }
            });
        link.into_iter()
            .chain(history)
            .chain(
                self.saved
                    .linking
                    .pages
                    .iter()
                    .filter(|(_, pending)| {
                        pending.answer.is_none()
                            && pending.failure.is_none()
                            && !self
                                .saved
                                .membership
                                .revoked
                                .contains(&self.account.primary)
                    })
                    .map(|(space, _)| self.linking_net.page_due.get(space).copied().unwrap_or(0)),
            )
            .min()
    }

    pub(super) fn linked_device_message(
        &mut self,
        from: &str,
        to: &str,
        sealed: &[u8],
        frames: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        if to != self.account.device
            || sealed.len() > MAX_DEVICE_MESSAGE
            || self.saved.membership.revoked.contains(from)
        {
            return Ok(());
        }
        let peer = if from == self.account.primary && !self.account.link_secret.is_empty() {
            Peer {
                device: from.into(),
                their_hpke: self.account.primary_hpke.clone(),
                my_hpke_secret: self.account.hpke_secret.clone(),
                secret: self.account.link_secret.clone(),
            }
        } else if let Some(peer) = self.saved.linking.peers.get(from) {
            peer.clone()
        } else {
            return Ok(());
        };
        match peer.open(sealed) {
            Some(DeviceMsg::PageRequest {
                req,
                space,
                before,
                limit,
            }) => {
                if !self.saved.linking.certified.contains(from)
                    || req.len() > 128
                    || !self.roster(&space).contains_key(&self.account.identity)
                    || self.saved.membership.revoked.contains(from)
                {
                    return Ok(());
                }
                let opened = self
                    .logs
                    .get(&space)
                    .map(|log| {
                        log.events()
                            .iter()
                            .filter(|event| event.seq < before && event.sealed_wire.is_some())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let start = opened
                    .len()
                    .saturating_sub((limit as usize).clamp(1, PAGE_LIMIT));
                let mut events = opened[start..]
                    .iter()
                    .map(|event| (*event).clone())
                    .collect::<Vec<_>>();
                // The relay bounds device messages. Bound actual serialized bytes too,
                // and keep the newest suffix so a large attachment cannot wedge paging.
                loop {
                    let msg = DeviceMsg::Page {
                        req: req.clone(),
                        space: space.clone(),
                        events: events.clone(),
                        more: start > 0 || events.len() < opened.len(),
                    };
                    let Some(sealed) = peer.seal(&msg) else {
                        break;
                    };
                    if sealed.len() <= MAX_DEVICE_MESSAGE {
                        frames.push(
                            ClientFrame::Req {
                                id: DEVICE_REQUEST,
                                op: Op::SendDevice {
                                    to: from.into(),
                                    sealed,
                                },
                            }
                            .encode(),
                        );
                        break;
                    }
                    if events.is_empty() {
                        break;
                    }
                    events.remove(0);
                }
            }
            Some(DeviceMsg::Page {
                req,
                space,
                events,
                more,
            }) => {
                let Some(pending) = self
                    .saved
                    .linking
                    .pages
                    .get(&space)
                    .filter(|pending| pending.req == req && pending.answer.is_none())
                else {
                    return Ok(());
                };
                if from != self.account.primary
                    || events.len() > PAGE_LIMIT
                    || events
                        .iter()
                        .any(|event| event.space != space || event.seq >= pending.before)
                {
                    return Ok(());
                }
                if events.is_empty() && more {
                    self.saved.linking.pages.get_mut(&space).unwrap().failure=Some("Esta página do histórico é grande demais para transferir. Abra a conversa no aparelho principal.".into());
                    return Ok(());
                }
                let Ok(loaded) = self.import_history_events(events) else {
                    return Ok(());
                };
                self.saved.linking.pages.get_mut(&space).unwrap().answer = Some((loaded, more));
                if !more {
                    self.saved.linking.complete.insert(space);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use event_log::{Author, Signer as Root};
    use openmls_traits::{signatures::SignerError, types::SignatureScheme};
    #[derive(Clone)]
    struct HostSigner(Root);
    impl Signer for HostSigner {
        fn sign(&self, value: &[u8]) -> Result<Vec<u8>, SignerError> {
            Ok(hex::decode(self.0.sign(value)).unwrap())
        }
        fn signature_scheme(&self) -> SignatureScheme {
            mls::SUITE.signature_algorithm()
        }
    }
    fn source() -> (Primary, Client<HostSigner>) {
        let signer = HostSigner(Root::from_secret(&[101; 32]));
        let primary =
            Primary::new(&signer.0.id(), "http://localhost:8790", "Private owner").unwrap();
        let mut client = Client::new(primary.account().clone(), signer, "").unwrap();
        client.synced = true;
        (primary, client)
    }
    #[test]
    fn retained_request_allocator_is_exact_u32_and_refuses_exhaustion_without_mutation() {
        let (_, mut client) = source();
        client.saved.linking.request_sequence = u64::from(u32::MAX) - LINK_REQUEST_BASE - 1;
        assert_eq!(
            client.next_retained_request_id().unwrap(),
            u64::from(u32::MAX)
        );
        let before = client.saved.linking.request_sequence;
        assert!(client.next_retained_request_id().is_err());
        assert_eq!(client.saved.linking.request_sequence, before);
        client.saved.linking.request_sequence = u64::MAX;
        assert!(client.next_retained_request_id().is_err());
        assert_eq!(client.saved.linking.request_sequence, u64::MAX);
    }

    fn prepare(
        primary: &Primary,
        client: &mut Client<HostSigner>,
        seed: u8,
    ) -> (crate::PendingLink, crate::LinkedAccount, u64) {
        let pending =
            crate::create_link(&Root::from_secret(&[seed; 32]).id(), &client.account.relay)
                .unwrap();
        let response = client.link_prepare(primary, &pending.code, 100).unwrap();
        let update = response.update.unwrap();
        let (id, sealed) = update
            .frames
            .iter()
            .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Req {
                    id,
                    op: Op::DeliverLink { sealed, .. },
                } => Some((id, sealed)),
                _ => None,
            })
            .unwrap();
        let account = crate::complete_link(&pending, &sealed).unwrap();
        (pending, account, id)
    }
    fn ack(client: &mut Client<HostSigner>, id: u64, now: i64) {
        client
            .receive(
                &ServerFrame::Res {
                    id,
                    result: Ok(Reply::Done),
                }
                .encode(),
                now,
            )
            .unwrap();
    }
    fn command(client: &mut Client<HostSigner>, request: Value, now: i64) -> DocumentResponse {
        client.link_request(&request.to_string(), now).unwrap()
    }

    #[test]
    fn handover_cold_retry_reuses_ciphertext_without_retaining_root_and_ignores_old_ack() {
        let (primary, mut client) = source();
        let (pending, linked, id) = prepare(&primary, &mut client, 102);
        assert_eq!(linked.identity, client.account.identity);
        assert_eq!(linked.primary, client.account.device);
        let first = client
            .saved
            .linking
            .handover
            .as_ref()
            .unwrap()
            .sealed
            .clone();
        let snapshot = client.update(Vec::new()).unwrap().snapshot;
        assert!(!snapshot.contains(&hex::encode(primary.authority_bytes())));
        assert!(!snapshot.contains("identity_secret"));
        let mut resumed =
            Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
        resumed.synced = true;
        let retry = resumed.link_prepare(&primary, &pending.code, 200).unwrap();
        assert_eq!(
            resumed.saved.linking.handover.as_ref().unwrap().sealed,
            first
        );
        assert!(retry.update.unwrap().frames.iter().any(|frame|matches!(ClientFrame::decode(frame),Ok(ClientFrame::Req{id:retry_id,..})if retry_id==id)));
        assert!(!resumed.saved.linking.certified.contains(&linked.device));
        ack(&mut resumed, id, 201);
        assert!(resumed.saved.linking.certified.contains(&linked.device));
        assert!(resumed
            .saved
            .linking
            .handover
            .as_ref()
            .unwrap()
            .sealed
            .is_empty());
        let (_, other, new_id) = prepare(&primary, &mut resumed, 103);
        assert_ne!(id, new_id);
        ack(&mut resumed, id, 202);
        assert!(!resumed.saved.linking.certified.contains(&other.device));
        assert!(!resumed.saved.linking.handover.as_ref().unwrap().delivered);
        let wrong = Primary::new(
            &Root::from_secret(&[104; 32]).id(),
            "http://localhost:8790",
            "Other",
        )
        .unwrap();
        assert!(wrong
            .seal_link(
                &device_link::LinkCode::parse(&pending.code).unwrap(),
                &client.account
            )
            .is_err());
    }

    #[test]
    fn outgoing_history_retains_exact_ciphertext_until_ack_and_native_wire_opens_it() {
        let (primary, mut client) = source();
        let (pending, linked, id) = prepare(&primary, &mut client, 105);
        assert!(client
            .link_request(
                &json!({"op":"history_send","device_id":linked.device}).to_string(),
                101
            )
            .is_err());
        ack(&mut client, id, 102);
        let first = command(
            &mut client,
            json!({"op":"history_send","device_id":linked.device}),
            103,
        );
        let work = first.result;
        let sealed: Vec<u8> = serde_json::from_value(work["bytes"].clone()).unwrap();
        assert_eq!(work["kind"], "upload");
        let payload = protocol::transfer_message(
            "put",
            work["transfer"].as_str().unwrap(),
            "0",
            work["sha256"].as_str().unwrap(),
            103,
        );
        assert!(event_log::verify_sig(
            &client.account.device,
            &payload,
            work["signature"].as_str().unwrap()
        ));
        let mut resumed = Client::new(
            client.account.clone(),
            client.signer.clone(),
            &first.update.unwrap().snapshot,
        )
        .unwrap();
        resumed.synced = true;
        let again = command(&mut resumed, json!({"op":"history_send_resume"}), 104).result;
        assert_eq!(again["bytes"], work["bytes"]);
        assert_eq!(again["transfer"], work["transfer"]);
        let transfer = work["transfer"].as_str().unwrap();
        let refused = command(
            &mut resumed,
            json!({"op":"history_failed","transfer":transfer,"message":"Server refused"}),
            104,
        );
        assert_eq!(refused.result["kind"], "failed");
        let snapshot = refused.update.unwrap().snapshot;
        resumed = Client::new(resumed.account.clone(), resumed.signer.clone(), &snapshot).unwrap();
        resumed.synced = true;
        assert!(resumed
            .link_request(&json!({"op":"history_send_resume"}).to_string(), 104)
            .is_err());
        let retried = command(
            &mut resumed,
            json!({"op":"history_send","device_id":linked.device}),
            104,
        )
        .result;
        assert_eq!(retried["bytes"], work["bytes"]);
        assert_eq!(retried["transfer"], work["transfer"]);

        assert!(resumed.link_request(&json!({"op":"history_uploaded","transfer":transfer,"index":0,"sha256":"00".repeat(32)}).to_string(),105).is_err());
        let uploaded = command(
            &mut resumed,
            json!({"op":"history_uploaded","transfer":transfer,"index":0,"sha256":work["sha256"]}),
            106,
        );
        let (request, manifest) = uploaded
            .update
            .unwrap()
            .frames
            .iter()
            .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Req {
                    id,
                    op:
                        Op::DeliverLink {
                            sealed,
                            device: None,
                            ..
                        },
                } => Some((id, sealed)),
                _ => None,
            })
            .unwrap();
        let plain = device_link::open(
            &hex::decode(&pending.hpke_secret).unwrap(),
            &hex::decode(&pending.secret).unwrap().try_into().unwrap(),
            "history",
            &manifest,
        )
        .unwrap();
        let manifest: device_link::HistoryBox = serde_json::from_slice(&plain).unwrap();
        assert_eq!(manifest.transfer, transfer);
        assert_eq!(manifest.chunks[0], event_log::sha256_hex(&sealed));
        let key = hex::decode(manifest.key).unwrap().try_into().unwrap();
        let bundle: device_link::Bundle =
            serde_json::from_slice(&device_link::open_chunk(&key, 0, &sealed).unwrap()).unwrap();
        assert!(bundle.events.is_empty());
        assert_eq!(
            command(&mut resumed, json!({"op":"history_send_resume"}), 107).result["kind"],
            "waiting"
        );
        resumed
            .saved
            .membership
            .revoked
            .insert(linked.device.clone());
        resumed.linking_net.history_due = 0;
        let after_revoke = resumed.maintenance(108).unwrap();
        assert!(!after_revoke.frames.iter().any(|frame| matches!(
            ClientFrame::decode(frame),
            Ok(ClientFrame::Req {
                op: Op::DeliverLink { device: None, .. },
                ..
            })
        )));
        assert!(resumed
            .link_request(&json!({"op":"history_send_resume"}).to_string(), 108)
            .is_err());
        resumed.saved.membership.revoked.clear();
        let delivered = resumed
            .receive(
                &ServerFrame::Res {
                    id: request,
                    result: Ok(Reply::Done),
                }
                .encode(),
                108,
            )
            .unwrap();
        let hint = delivered
            .frames
            .iter()
            .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Req {
                    op: Op::SendDevice { to, sealed },
                    ..
                } if to == linked.device => Some(sealed),
                _ => None,
            })
            .unwrap();
        let receiver = Peer {
            device: resumed.account.device.clone(),
            their_hpke: String::new(),
            my_hpke_secret: pending.hpke_secret.clone(),
            secret: pending.secret.clone(),
        };
        assert!(matches!(
            receiver.open(&hint),
            Some(DeviceMsg::HistoryReady)
        ));
        assert_eq!(
            command(&mut resumed, json!({"op":"history_send_resume"}), 109).result["kind"],
            "complete"
        );
        assert!(resumed
            .saved
            .linking
            .sending
            .as_ref()
            .unwrap()
            .chunks
            .is_empty());
    }

    #[test]
    fn older_page_requires_matching_peer_request_space_and_signed_relay_record() {
        let (primary, mut source) = source();
        let (_, linked, id) = prepare(&primary, &mut source, 106);
        ack(&mut source, id, 101);
        let identity = Root::from_secret(&primary.authority_bytes().try_into().unwrap());
        let author = Author::device(&identity, source.signer.0.clone());
        let space = "sp_older";
        let mut log = SpaceLog::new(space);
        log.append(
            &identity,
            1,
            EventBody::SpaceCreated {
                title: "Private".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        );
        let mut opened = author.sign_event(
            space,
            "msg_old",
            2,
            log.head(),
            EventBody::MessagePosted {
                message: "msg_old".into(),
                text: "Actual old message".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        );
        let (content, sig) = author.sign_sealed(
            space,
            "msg_old",
            2,
            log.head().as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, vec![1]),
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
        let outer = log.sequence(outer).clone();
        opened.seq = outer.seq;
        opened.sealed_wire = Some(event_log::content_hash_of(&outer));
        let opened = device_link::opened_history_event(&outer, opened).unwrap();
        let mut source_events = log.events().to_vec();
        source_events[1] = opened.clone();
        source.logs.insert(
            space.into(),
            SpaceLog::from_events(space, source_events).unwrap(),
        );
        let mut destination = Client::new(
            linked.clone(),
            HostSigner(Root::from_secret(&[106; 32])),
            "",
        )
        .unwrap();
        destination.synced = true;
        destination.logs.insert(space.into(), log);
        let request = command(
            &mut destination,
            json!({"op":"load_older","space_id":space}),
            200,
        );
        let req = request.result["request_id"].as_str().unwrap();
        let sealed = request
            .update
            .unwrap()
            .frames
            .iter()
            .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Req {
                    op: Op::SendDevice { sealed, .. },
                    ..
                } => Some(sealed),
                _ => None,
            })
            .unwrap();
        let answer = source
            .receive(
                &ServerFrame::DeviceMessage {
                    from: linked.device.clone(),
                    to: source.account.device.clone(),
                    sealed,
                }
                .encode(),
                201,
            )
            .unwrap();
        let sealed = answer
            .frames
            .iter()
            .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Req {
                    op: Op::SendDevice { sealed, .. },
                    ..
                } => Some(sealed),
                _ => None,
            })
            .unwrap();
        // A valid HPKE sender still cannot substitute a different Space or request.
        let peer = &source.saved.linking.peers[&linked.device];
        for (request, space) in [("other", space), (req, "sp_other")] {
            let sealed = peer
                .seal(&DeviceMsg::Page {
                    req: request.into(),
                    space: space.into(),
                    events: vec![opened.clone()],
                    more: false,
                })
                .unwrap();
            destination
                .receive(
                    &ServerFrame::DeviceMessage {
                        from: source.account.device.clone(),
                        to: linked.device.clone(),
                        sealed,
                    }
                    .encode(),
                    202,
                )
                .unwrap();
            assert!(destination.saved.linking.pages["sp_older"].answer.is_none());
        }
        destination
            .receive(
                &ServerFrame::DeviceMessage {
                    from: source.account.device.clone(),
                    to: linked.device.clone(),
                    sealed,
                }
                .encode(),
                203,
            )
            .unwrap();
        assert_eq!(
            destination.saved.linking.pages[space].answer,
            Some((1, false))
        );
        assert!(destination.saved.linking.complete.contains(space));
        let snapshot = destination.update(Vec::new()).unwrap().snapshot;
        let resumed = Client::new(
            linked.clone(),
            HostSigner(Root::from_secret(&[106; 32])),
            &snapshot,
        )
        .unwrap();
        assert!(
            matches!(&resumed.logs[space].events()[1].body,EventBody::MessagePosted{text,..}if text=="Actual old message")
        );
        assert!(resumed.saved.linking.complete.contains(space));

        // JSON escaping may make one legitimate opened record larger than the
        // authenticated device-channel budget. Report it instead of retrying the
        // same empty boundary forever or claiming the history is complete.
        let mut large = author.sign_event(
            space,
            "msg_old",
            2,
            outer.seen.clone(),
            EventBody::MessagePosted {
                message: "msg_old".into(),
                text: "\n".repeat(600_000),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        );
        large.seq = outer.seq;
        large.sealed_wire = Some(event_log::content_hash_of(&outer));
        let large = device_link::opened_history_event(&outer, large).unwrap();
        let create = source.logs[space].events()[0].clone();
        source.logs.insert(
            space.into(),
            SpaceLog::from_events(space, vec![create.clone(), large]).unwrap(),
        );
        let mut destination = Client::new(
            linked.clone(),
            HostSigner(Root::from_secret(&[106; 32])),
            "",
        )
        .unwrap();
        destination.synced = true;
        destination.logs.insert(
            space.into(),
            SpaceLog::from_events(space, vec![create, outer]).unwrap(),
        );
        let request = command(
            &mut destination,
            json!({"op":"load_older","space_id":space}),
            400,
        );
        let req = request.result["request_id"].as_str().unwrap();
        let sealed = request
            .update
            .unwrap()
            .frames
            .iter()
            .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Req {
                    op: Op::SendDevice { sealed, .. },
                    ..
                } => Some(sealed),
                _ => None,
            })
            .unwrap();
        let response = source
            .receive(
                &ServerFrame::DeviceMessage {
                    from: linked.device.clone(),
                    to: source.account.device.clone(),
                    sealed,
                }
                .encode(),
                401,
            )
            .unwrap();
        let sealed = response
            .frames
            .iter()
            .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Req {
                    op: Op::SendDevice { sealed, .. },
                    ..
                } => Some(sealed),
                _ => None,
            })
            .unwrap();
        let update = destination
            .receive(
                &ServerFrame::DeviceMessage {
                    from: source.account.device.clone(),
                    to: linked.device,
                    sealed,
                }
                .encode(),
                402,
            )
            .unwrap();
        assert!(!destination.saved.linking.complete.contains(space));
        let status = command(
            &mut destination,
            json!({"op":"older_status","space_id":space,"request_id":req}),
            403,
        )
        .result;
        assert_eq!(status["pending"], false);
        assert!(status["failure"]
            .as_str()
            .unwrap()
            .contains("grande demais"));
        assert!(update.maintenance_ms.is_none_or(|deadline| deadline > 0));
        assert!(!destination
            .maintenance(10_000)
            .unwrap()
            .frames
            .iter()
            .any(|frame| matches!(
                ClientFrame::decode(frame),
                Ok(ClientFrame::Req {
                    op: Op::SendDevice { .. },
                    ..
                })
            )));
    }
}
