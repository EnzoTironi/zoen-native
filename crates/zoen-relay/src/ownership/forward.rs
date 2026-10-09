//! Cell-private request/reply forwarding. Only the receiving owner can commit;
//! forwarding never re-routes at the destination, so routing loops are impossible.

use async_nats::{Client, HeaderMap};
use bytes::Bytes;
use futures_util::StreamExt;
use roda_proto::{ClientFrame, Envelope, InviteCreated, Sequenced};
use roda_types::{EventBody, Role};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Semaphore, task::JoinHandle};

use super::Fence;
use crate::log::{fdb::FdbLog, Reject, Sequencing};

const FENCE_HEADER: &str = "Zoen-Owner-Fence";
pub const MAX_REQUESTS: usize = 64;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
pub const SUBSCRIPTION_CAPACITY: usize = 32;
pub const RPC_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_METADATA: usize = 256 * 1024;

pub struct Forwarder {
    client: Client,
    prefix: String,
    count: Arc<Semaphore>,
    bytes: Arc<Semaphore>,
    listener: JoinHandle<()>,
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "result")]
enum Answer {
    Invite {
        code: String,
        expires_at_ms: i64,
    },
    New {
        audience: Vec<String>,
        joined: Option<String>,
    },
    Duplicate,
    Rejected {
        reason: String,
        permanent: bool,
    },
}

#[derive(Serialize, Deserialize)]
struct Request {
    fence: Fence,
    invite: Option<InviteRequest>,
}

#[derive(Serialize, Deserialize)]
struct InviteRequest {
    who: String,
    space: String,
    role: Role,
    max_uses: u32,
    ttl_secs: u64,
    code: String,
}

fn subject_prefix(cell: &str) -> String {
    format!(
        "zoen.{}.owner",
        hex::encode(Sha256::digest(cell.as_bytes()))
    )
}

fn subject(prefix: &str, node: &str) -> String {
    format!("{prefix}.{}", hex::encode(Sha256::digest(node.as_bytes())))
}

fn header_bytes(headers: &HeaderMap) -> usize {
    12 + headers
        .iter()
        .map(|(name, values)| {
            let name: &str = name.as_ref();
            values
                .iter()
                .map(|value| name.len() + value.as_str().len() + 4)
                .sum::<usize>()
        })
        .sum::<usize>()
}

impl Forwarder {
    pub fn connected(&self) -> bool {
        self.client.connection_state() == async_nats::connection::State::Connected
    }
    pub async fn connect(
        log: &Arc<FdbLog>,
        url: &str,
        cell: &str,
        pool: sqlx::PgPool,
    ) -> anyhow::Result<Self> {
        let client = async_nats::ConnectOptions::new()
            .client_capacity(128)
            .subscription_capacity(SUBSCRIPTION_CAPACITY)
            .request_timeout(Some(RPC_TIMEOUT))
            .connect(url)
            .await?;
        let prefix = subject_prefix(cell);
        let mut sub = client.subscribe(subject(&prefix, &log.owner.node)).await?;
        client.flush().await?;
        let weak = Arc::downgrade(log);
        let sender = client.clone();
        let count = Arc::new(Semaphore::new(MAX_REQUESTS));
        let bytes = Arc::new(Semaphore::new(MAX_BYTES));
        let (in_count, in_bytes) = (count.clone(), bytes.clone());
        let listener = tokio::spawn(async move {
            while let Some(msg) = sub.next().await {
                let Some(reply) = msg.reply else { continue };
                let size = msg
                    .payload
                    .len()
                    .saturating_add(msg.headers.as_ref().map_or(0, header_bytes));
                let permits = in_count.clone().try_acquire_owned().ok().zip(
                    in_bytes
                        .clone()
                        .try_acquire_many_owned(size.min(u32::MAX as usize) as u32)
                        .ok(),
                );
                let Some(permits) = permits else {
                    let _ = tokio::time::timeout(
                        RPC_TIMEOUT,
                        sender.publish(
                            reply,
                            encode(Err(Reject::retry("owner forwarding capacity reached"))),
                        ),
                    )
                    .await;
                    continue;
                };
                let Some(log) = weak.upgrade() else { break };
                let (sender, pool) = (sender.clone(), pool.clone());
                tokio::spawn(async move {
                    let fence = msg
                        .headers
                        .as_ref()
                        .and_then(|h| h.get(FENCE_HEADER))
                        .and_then(|h| serde_json::from_str::<Request>(h.as_str()).ok());
                    let response = if size > crate::session::MAX_FRAME + 1024 {
                        encode(Err(Reject::no("invalid owner forwarding request")))
                    } else if let Some(Request {
                        fence,
                        invite: Some(invite),
                    }) = fence
                    {
                        let result = log
                            .create_invite_fenced(
                                &invite.who,
                                &invite.space,
                                invite.role,
                                invite.max_uses,
                                invite.ttl_secs,
                                &invite.code,
                                fence,
                            )
                            .await;
                        let meta = match result {
                            Ok(inv) => Answer::Invite {
                                code: inv.code,
                                expires_at_ms: inv.expires_at_ms,
                            },
                            Err(reason) => Answer::Rejected {
                                reason,
                                permanent: false,
                            },
                        };
                        encode_parts(meta, None)
                    } else {
                        let result = match (fence, ClientFrame::decode(&msg.payload)) {
                            (
                                Some(Request {
                                    fence,
                                    invite: None,
                                }),
                                Ok(ClientFrame::Publish { env }),
                            ) => admit(&log, &pool, env, fence).await,
                            _ => Err(Reject::no("invalid owner forwarding request")),
                        };
                        encode(result)
                    };
                    let _ =
                        tokio::time::timeout(RPC_TIMEOUT, sender.publish(reply, response)).await;
                    drop(permits);
                });
            }
        });
        Ok(Self {
            client,
            prefix,
            count,
            bytes,
            listener,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn invite(
        &self,
        who: &str,
        space: &str,
        role: Role,
        max_uses: u32,
        ttl_secs: u64,
        code: &str,
        fence: &Fence,
    ) -> Result<InviteCreated, String> {
        let _count = self
            .count
            .clone()
            .try_acquire_owned()
            .map_err(|_| "owner forwarding capacity reached".to_string())?;
        // Reserve before cloning or JSON encoding; a string can expand sixfold.
        let size = [who, space, code, &fence.owner]
            .iter()
            .fold(512usize, |bytes, value| {
                bytes.saturating_add(value.len().saturating_mul(6))
            });
        let _bytes = self
            .bytes
            .clone()
            .try_acquire_many_owned(size.min(u32::MAX as usize) as u32)
            .map_err(|_| "owner forwarding capacity reached".to_string())?;
        let request = Request {
            fence: fence.clone(),
            invite: Some(InviteRequest {
                who: who.into(),
                space: space.into(),
                role,
                max_uses,
                ttl_secs,
                code: code.into(),
            }),
        };
        let encoded = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        let mut headers = HeaderMap::new();
        headers.insert(FENCE_HEADER, encoded.as_str());
        if header_bytes(&headers) > self.client.server_info().max_payload {
            return Err("invite exceeds cell forwarding payload limit".into());
        }
        let response = tokio::time::timeout(
            RPC_TIMEOUT,
            self.client.request_with_headers(
                subject(&self.prefix, &fence.owner),
                headers,
                Bytes::new(),
            ),
        )
        .await
        .map_err(|_| "partition owner did not answer".to_string())?
        .map_err(|_| "partition owner unavailable".to_string())?;
        match decode_parts(&response.payload).map_err(|r| r.reason)?.0 {
            Answer::Invite {
                code,
                expires_at_ms,
            } => Ok(InviteCreated {
                code,
                expires_at_ms,
            }),
            Answer::Rejected { reason, .. } => Err(reason),
            _ => Err("invalid invite response".into()),
        }
    }

    pub async fn append(&self, env: &Envelope, fence: &Fence) -> Result<Sequencing, Reject> {
        let _count = self
            .count
            .clone()
            .try_acquire_owned()
            .map_err(|_| Reject::retry("owner forwarding capacity reached"))?;
        let size = env.stored_len().saturating_add(1024);
        let _bytes = self
            .bytes
            .clone()
            .try_acquire_many_owned(size.min(u32::MAX as usize) as u32)
            .map_err(|_| Reject::retry("owner forwarding capacity reached"))?;
        let mut headers = HeaderMap::new();
        let fence_json = serde_json::to_string(&Request {
            fence: fence.clone(),
            invite: None,
        })
        .map_err(|_| Reject::unavailable())?;
        headers.insert(FENCE_HEADER, fence_json.as_str());
        let wire = ClientFrame::Publish { env: env.clone() }.encode();
        if wire.len() + header_bytes(&headers) > self.client.server_info().max_payload {
            return Err(Reject::no("event exceeds cell forwarding payload limit"));
        }
        let answer = tokio::time::timeout(
            RPC_TIMEOUT,
            self.client.request_with_headers(
                subject(&self.prefix, &fence.owner),
                headers,
                Bytes::from(wire),
            ),
        )
        .await
        .map_err(|_| Reject::retry("partition owner did not answer"))?
        .map_err(|_| Reject::retry("partition owner unavailable"))?;
        decode(&answer.payload)
    }
}

async fn admit(
    log: &FdbLog,
    pool: &sqlx::PgPool,
    env: Envelope,
    fence: Fence,
) -> Result<Sequencing, Reject> {
    if env.verify().is_err() {
        return Err(Reject::no("invalid forwarded signature"));
    }
    // The cell bus is private, but a forwarded device is still checked at the owner.
    if let Some(device) = env.device() {
        let auth = crate::db::authorize_device(pool, env.author(), device, true)
            .await
            .map_err(|_| Reject::unavailable())?
            .ok_or_else(|| Reject::no("forwarded device is not authorized"))?;
        auth.commit().await.map_err(|_| Reject::unavailable())?;
    } else {
        return Err(Reject::no("forwarded publish requires an enrolled device"));
    }
    let known = match env.body() {
        Some(EventBody::MemberAdded { identity, .. }) => crate::db::is_registered(pool, &identity)
            .await
            .map_err(|_| Reject::unavailable())?,
        _ => true,
    };
    log.append_fenced(&env, known, fence).await
}

fn encode(result: Result<Sequencing, Reject>) -> Bytes {
    let (meta, event) = match result {
        Ok(Sequencing::New {
            ev,
            audience,
            joined,
        }) => (Answer::New { audience, joined }, Some(ev)),
        Ok(Sequencing::Duplicate { ev }) => (Answer::Duplicate, Some(ev)),
        Err(r) => (
            Answer::Rejected {
                reason: r.reason,
                permanent: r.permanent,
            },
            None,
        ),
    };
    encode_parts(meta, event)
}

fn encode_parts(meta: Answer, event: Option<Sequenced>) -> Bytes {
    let meta = serde_json::to_vec(&meta).expect("forward response metadata");
    let mut out =
        Vec::with_capacity(meta.len() + event.as_ref().map_or(0, |e| e.env.stored_len()) + 4);
    out.extend_from_slice(&(meta.len() as u32).to_be_bytes());
    out.extend_from_slice(&meta);
    if let Some(event) = event {
        out.extend_from_slice(&event.encode());
    }
    Bytes::from(out)
}

fn decode_parts(bytes: &[u8]) -> Result<(Answer, &[u8]), Reject> {
    let prefix: [u8; 4] = bytes
        .get(..4)
        .and_then(|v| v.try_into().ok())
        .ok_or_else(Reject::unavailable)?;
    let size = u32::from_be_bytes(prefix) as usize;
    if size > MAX_METADATA {
        return Err(Reject::unavailable());
    }
    let meta = bytes.get(4..4 + size).ok_or_else(Reject::unavailable)?;
    let meta: Answer = serde_json::from_slice(meta).map_err(|_| Reject::unavailable())?;
    Ok((meta, &bytes[4 + size..]))
}

fn decode(bytes: &[u8]) -> Result<Sequencing, Reject> {
    let (meta, event) = decode_parts(bytes)?;
    if let Answer::Rejected { reason, permanent } = meta {
        return Err(Reject { reason, permanent });
    }
    let ev = Sequenced::decode(event).map_err(|_| Reject::unavailable())?;
    match meta {
        Answer::New { audience, joined } => Ok(Sequencing::New {
            ev,
            audience,
            joined,
        }),
        Answer::Duplicate => Ok(Sequencing::Duplicate { ev }),
        Answer::Invite { .. } => Err(Reject::unavailable()),
        Answer::Rejected { .. } => unreachable!(),
    }
}
