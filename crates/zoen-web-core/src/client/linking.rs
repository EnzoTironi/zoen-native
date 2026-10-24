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
                    let pending = requests::retain_page(
                        &mut self.saved.linking.pages,
                        &space_id,
                        before,
                        || browser_id("pg", now),
                    )?;
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
mod tests;

mod delivery;

mod requests;
