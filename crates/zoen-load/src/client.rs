//! One simulated person: a device that logs in over the real protocol, registers, joins
//! one group and then publishes signed messages on command while recording what it sees.

use std::{collections::HashMap, time::Duration};

use anyhow::{anyhow, bail, Context};
use futures_util::{SinkExt, StreamExt};
use hdrhistogram::Histogram;
use roda_log::{Author, Signer};
use roda_proto::{
    auth_message, ClientFrame, Envelope, Op, Sequenced, ServerFrame, PROTOCOL_VERSION,
};
use roda_types::{EventBody, Identity, IdentityKind, Privacy, Role, Seen, SpaceKind};
use tokio::{net::TcpStream, sync::mpsc, time::Instant};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Latencies are recorded in microseconds, up to a minute.
pub fn histogram() -> Histogram<u64> {
    Histogram::new_with_bounds(1, 60_000_000, 3).expect("histogram bounds")
}

pub struct Conn {
    ws: Ws,
    pub author: Author,
}

impl Conn {
    /// TCP + WebSocket + Hello/Challenge/Auth + Register.
    pub async fn open(relay: &str) -> anyhow::Result<Conn> {
        let url = format!("{}/v1/sync", relay.replace("http://", "ws://"));
        let (ws, _) = tokio_tungstenite::connect_async_with_config(url.as_str(), None, true)
            .await
            .context("websocket")?;
        let root = Signer::generate();
        let author = Author::device(&root, Signer::generate());
        let mut c = Conn { ws, author };
        c.send(&ClientFrame::Hello {
            protocol: PROTOCOL_VERSION,
            capabilities: Vec::new(),
            identity: root.id(),
            device: c.author.device.clone().expect("device"),
            cert: c.author.cert.clone().expect("cert"),
        })
        .await?;
        let (nonce, relay_name) = match c.recv().await? {
            ServerFrame::Challenge { nonce, relay, .. } => (nonce, relay),
            ServerFrame::Error { message, .. } => bail!("refused: {message}"),
            other => bail!("expected challenge, got {other:?}"),
        };
        let sig = c.author.key.sign(&auth_message(&nonce, &relay_name));
        c.send(&ClientFrame::Auth { sig }).await?;
        match c.recv().await? {
            ServerFrame::Ready { .. } => {}
            ServerFrame::Error { message, .. } => bail!("refused: {message}"),
            other => bail!("expected ready, got {other:?}"),
        }
        let handle = format!("l{}", &root.id()[..16]);
        c.send(&ClientFrame::Req {
            id: 1,
            op: Op::Register {
                profile: Identity {
                    id: root.id(),
                    kind: IdentityKind::Person,
                    name: String::new(),
                    handle,
                    tint_hex: "#000".into(),
                    glyph: None,
                    owner: None,
                    bio: String::new(),
                },
            },
        })
        .await?;
        loop {
            if let ServerFrame::Res { id: 1, result } = c.recv().await? {
                result.map_err(|e| anyhow!("register: {e}"))?;
                return Ok(c);
            }
        }
    }

    pub fn identity(&self) -> &str {
        &self.author.identity
    }

    async fn send(&mut self, f: &ClientFrame) -> anyhow::Result<()> {
        self.ws.send(Message::Binary(f.encode().into())).await?;
        Ok(())
    }

    async fn recv(&mut self) -> anyhow::Result<ServerFrame> {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(30), self.ws.next())
                .await
                .context("relay silent for 30 s")?
                .ok_or_else(|| anyhow!("relay closed the connection"))??;
            if let Message::Binary(b) = msg {
                return ServerFrame::decode(&b).map_err(|e| anyhow!("undecodable frame: {e}"));
            }
        }
    }

    /// Signs `body` on top of `seen` and waits for its sequenced echo.
    async fn publish(
        &mut self,
        space: &str,
        seen: Option<Seen>,
        body: EventBody,
    ) -> anyhow::Result<Sequenced> {
        let now = now_ms();
        let e = self
            .author
            .sign_event(space, &roda_types::new_ulid(now), now, seen, body);
        let env = Envelope::plain(&e);
        let client_id = env.client_id().to_string();
        self.send(&ClientFrame::Publish { env }).await?;
        loop {
            match self.recv().await? {
                ServerFrame::Event { ev } if ev.env.client_id() == client_id => return Ok(ev),
                ServerFrame::Rejected {
                    client_id: c,
                    reason,
                    ..
                } if c == client_id => {
                    bail!("rejected: {reason}")
                }
                _ => {}
            }
        }
    }

    /// Creates a group Space with `members` in it; returns its id.
    pub async fn create_group(&mut self, members: &[String]) -> anyhow::Result<String> {
        let space = roda_types::new_id("sp");
        let mut head = self
            .publish(
                &space,
                None,
                EventBody::SpaceCreated {
                    title: "load".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            )
            .await?;
        for m in members {
            head = self
                .publish(
                    &space,
                    Some(seen(&head)),
                    EventBody::MemberAdded {
                        identity: m.clone(),
                        role: Role::Member,
                    },
                )
                .await?;
        }
        Ok(space)
    }

    /// Catches up on every Space this identity is in; returns the newest entry of `space`.
    pub async fn catch_up(&mut self, space: &str) -> anyhow::Result<Seen> {
        self.send(&ClientFrame::Sync {
            cursors: Vec::new(),
            all: true,
        })
        .await?;
        let mut newest = None;
        loop {
            match self.recv().await? {
                ServerFrame::Event { ev } if ev.env.space() == space => newest = Some(seen(&ev)),
                ServerFrame::SyncDone => break,
                _ => {}
            }
        }
        newest.ok_or_else(|| anyhow!("not in the space after setup"))
    }
}

fn seen(ev: &Sequenced) -> Seen {
    Seen {
        seq: ev.seq,
        hash: ev.hash.clone(),
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// A message to publish, stamped with the time it was scheduled to go out (µs after the
/// run's start). Latency counts from that moment, so a slow relay can't hide queueing
/// (no coordinated omission).
pub struct Publish {
    pub due_us: u64,
}

pub struct Tally {
    pub sent: u64,
    pub accepted: u64,
    pub delivered: u64,
    pub rejected: HashMap<String, u64>,
    pub ack: Histogram<u64>,
    pub delivery: Histogram<u64>,
}

impl Tally {
    pub fn new() -> Tally {
        Tally {
            sent: 0,
            accepted: 0,
            delivered: 0,
            rejected: HashMap::new(),
            ack: histogram(),
            delivery: histogram(),
        }
    }

    pub fn merge(&mut self, o: Tally) {
        self.sent += o.sent;
        self.accepted += o.accepted;
        self.delivered += o.delivered;
        for (k, v) in o.rejected {
            *self.rejected.entry(k).or_default() += v;
        }
        let _ = self.ack.add(&o.ack);
        let _ = self.delivery.add(&o.delivery);
    }
}

/// What a person runs with.
pub struct Plan {
    pub space: String,
    pub head: Seen,
    pub start: Instant,
    /// Messages due before this (the warm-up) aren't recorded.
    pub measure_from_us: u64,
    /// How long to keep reading after the last command, for late acks and deliveries.
    pub drain: Duration,
}

/// Runs one person: publishes on command and records acks and deliveries, until `rx` closes
/// and the drain window has passed.
pub async fn run(conn: Conn, plan: Plan, mut rx: mpsc::Receiver<Publish>) -> anyhow::Result<Tally> {
    let Conn { ws, author } = conn;
    let Plan {
        space,
        mut head,
        start,
        measure_from_us,
        drain,
    } = plan;
    let (mut sink, mut stream) = ws.split();
    let mut pending: HashMap<String, u64> = HashMap::new();
    let mut t = Tally::new();
    let mut stop_at: Option<Instant> = None;
    loop {
        let frame = tokio::select! {
            cmd = rx.recv(), if stop_at.is_none() => {
                let Some(Publish { due_us }) = cmd else {
                    stop_at = Some(Instant::now() + drain);
                    continue;
                };
                let now = now_ms();
                let e = author.sign_event(
                    &space,
                    &roda_types::new_ulid(now),
                    now,
                    Some(head.clone()),
                    EventBody::MessagePosted {
                        message: roda_types::new_id("m"),
                        text: due_us.to_string(),
                        attaches: None,
                    },
                );
                let env = Envelope::plain(&e);
                if due_us >= measure_from_us {
                    pending.insert(env.client_id().to_string(), due_us);
                    t.sent += 1;
                }
                sink.send(Message::Binary(ClientFrame::Publish { env }.encode().into())).await?;
                continue;
            }
            _ = tokio::time::sleep_until(stop_at.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600))), if stop_at.is_some() => break,
            msg = stream.next() => msg,
        };
        let Some(msg) = frame else {
            bail!("relay closed the connection")
        };
        let Message::Binary(b) = msg? else { continue };
        let at_us = start.elapsed().as_micros() as u64;
        match ServerFrame::decode(&b).map_err(|e| anyhow!("undecodable frame: {e}"))? {
            ServerFrame::Event { ev } => {
                if ev.seq > head.seq {
                    head = seen(&ev);
                }
                if ev.env.author() == author.identity {
                    continue;
                }
                if let Some(EventBody::MessagePosted { text, .. }) = ev.env.body() {
                    if let Ok(due_us) = text.parse::<u64>() {
                        if due_us >= measure_from_us {
                            t.delivered += 1;
                            t.delivery
                                .saturating_record(at_us.saturating_sub(due_us).max(1));
                        }
                    }
                }
            }
            ServerFrame::Accepted { client_id, .. } => {
                if let Some(due_us) = pending.remove(&client_id) {
                    t.accepted += 1;
                    t.ack.saturating_record(at_us.saturating_sub(due_us).max(1));
                }
            }
            ServerFrame::Rejected {
                client_id, reason, ..
            } if pending.remove(&client_id).is_some() => {
                *t.rejected.entry(reason).or_default() += 1;
            }
            _ => {}
        }
    }
    let _ = sink.close().await;
    Ok(t)
}
