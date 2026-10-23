//! Bounded transport for a retained native publication. This uses the same
//! Hello/Auth, blob PUT and Publish protocol as Net, without owning any state.
use crate::blob_transport::{BlobUploadError, BlobUploadReceipt};
use event_log::Author;
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientFrame, Envelope, ServerFrame};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    tungstenite::{protocol::WebSocketConfig, Message},
    MaybeTlsStream, WebSocketStream,
};

#[derive(Debug, thiserror::Error)]
pub enum RelayOutputError {
    #[error("invalid relay output transport")]
    Invalid,
    #[error("relay output transport unavailable or acknowledgement uncertain")]
    Unavailable,
    #[error("relay output refused")]
    Refused,
    #[error(transparent)]
    Blob(#[from] BlobUploadError),
}

pub struct RelayOutputTransport {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    http: reqwest::Client,
    base: String,
    relay: String,
    actor: Author,
}

impl RelayOutputTransport {
    /// Only HTTPS/WSS or explicit numeric loopback development endpoints.
    /// Credentials are supplied by the native vault owner, never by an App.
    pub async fn connect(
        url: &str,
        expected_relay: &str,
        actor: Author,
    ) -> Result<Self, RelayOutputError> {
        let base = crate::net::http_base(url);
        let parsed = reqwest::Url::parse(&base).map_err(|_| RelayOutputError::Invalid)?;
        let loopback = parsed.host_str().is_some_and(|host| {
            host.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        });
        if !(parsed.scheme() == "https" || parsed.scheme() == "http" && loopback)
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
            || actor.device.as_deref() != Some(actor.key.id().as_str())
            || actor.cert.as_deref().is_none_or(str::is_empty)
        {
            return Err(RelayOutputError::Invalid);
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = WebSocketConfig::default()
            .max_message_size(Some(98_304))
            .max_frame_size(Some(98_304));
        let (socket, _) = tokio::time::timeout(
            Duration::from_secs(10),
            tokio_tungstenite::connect_async_with_config(
                crate::net::ws_url(&base),
                Some(config),
                true,
            ),
        )
        .await
        .map_err(|_| RelayOutputError::Unavailable)?
        .map_err(|_| RelayOutputError::Unavailable)?;
        let http = crate::https::client()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| RelayOutputError::Unavailable)?;
        let mut transport = Self {
            socket,
            http,
            base,
            relay: String::new(),
            actor,
        };
        tokio::time::timeout(
            Duration::from_secs(15),
            transport.authenticate(expected_relay),
        )
        .await
        .map_err(|_| RelayOutputError::Unavailable)??;
        Ok(transport)
    }

    async fn authenticate(&mut self, expected_relay: &str) -> Result<(), RelayOutputError> {
        self.send(ClientFrame::Hello {
            protocol: protocol::PROTOCOL_VERSION,
            capabilities: protocol::CAPABILITIES.iter().map(|c| (*c).into()).collect(),
            identity: self.actor.identity.clone(),
            device: self.actor.key.id(),
            cert: self.actor.cert.clone().ok_or(RelayOutputError::Invalid)?,
        })
        .await?;
        let ServerFrame::Challenge {
            nonce,
            relay,
            protocol,
            ..
        } = self.receive().await?
        else {
            return Err(RelayOutputError::Refused);
        };
        if protocol < protocol::MIN_PROTOCOL_VERSION
            || relay.is_empty()
            || relay.len() > 256
            || relay != expected_relay
        {
            return Err(RelayOutputError::Refused);
        }
        self.send(ClientFrame::Auth {
            sig: self.actor.key.sign(&protocol::auth_message(&nonce, &relay)),
        })
        .await?;
        for _ in 0..128 {
            match self.receive().await? {
                ServerFrame::Ready {
                    identity,
                    registered: true,
                } if identity == self.actor.identity => {
                    self.relay = relay;
                    return Ok(());
                }
                ServerFrame::Presence { .. } => {}
                _ => return Err(RelayOutputError::Refused),
            }
        }
        Err(RelayOutputError::Refused)
    }

    pub async fn upload(
        &self,
        sha: &str,
        bytes: Vec<u8>,
        at_ms: i64,
    ) -> Result<BlobUploadReceipt, RelayOutputError> {
        crate::blob_transport::put(
            &self.http,
            &self.base,
            &self.relay,
            &self.actor.key,
            sha,
            bytes,
            at_ms,
        )
        .await
        .map_err(Into::into)
    }

    /// An Accepted frame only identifies a position. The owner must read that
    /// position from the configured relay source and ingest its actual envelope.
    pub async fn publish(&mut self, env: &Envelope) -> Result<u64, RelayOutputError> {
        if env.verify().is_err()
            || env.author() != self.actor.identity
            || env.device() != self.actor.device.as_deref()
            || env.cert != self.actor.cert
            || !env.is_sealed()
            || env.is_pruned()
            || env.content().len() > 65_536
        {
            return Err(RelayOutputError::Invalid);
        }
        self.send(ClientFrame::Publish { env: env.clone() }).await?;
        tokio::time::timeout(Duration::from_secs(15), self.acceptance(env))
            .await
            .map_err(|_| RelayOutputError::Unavailable)?
    }

    async fn acceptance(&mut self, env: &Envelope) -> Result<u64, RelayOutputError> {
        for _ in 0..128 {
            match self.receive().await? {
                ServerFrame::Accepted {
                    space,
                    client_id,
                    seq,
                } if space == env.space() && client_id == env.client_id() => return Ok(seq),
                ServerFrame::Rejected {
                    space, client_id, ..
                } if space == env.space() && client_id == env.client_id() => {
                    return Err(RelayOutputError::Refused)
                }
                ServerFrame::Error { .. } => return Err(RelayOutputError::Refused),
                ServerFrame::Event { .. }
                | ServerFrame::Presence { .. }
                | ServerFrame::KeyPackagesLow { .. }
                | ServerFrame::ProfileChanged { .. }
                | ServerFrame::Ephemeral { .. }
                | ServerFrame::Joined { .. } => {}
                _ => return Err(RelayOutputError::Invalid),
            }
        }
        Err(RelayOutputError::Unavailable)
    }

    async fn send(&mut self, frame: ClientFrame) -> Result<(), RelayOutputError> {
        tokio::time::timeout(
            Duration::from_secs(10),
            self.socket.send(Message::Binary(frame.encode().into())),
        )
        .await
        .map_err(|_| RelayOutputError::Unavailable)?
        .map_err(|_| RelayOutputError::Unavailable)
    }
    async fn receive(&mut self) -> Result<ServerFrame, RelayOutputError> {
        tokio::time::timeout(Duration::from_secs(15), async {
            for _ in 0..128 {
                match self.socket.next().await {
                    Some(Ok(Message::Binary(bytes))) => {
                        return ServerFrame::decode(&bytes).map_err(|_| RelayOutputError::Invalid)
                    }
                    Some(Ok(Message::Ping(bytes))) => self
                        .socket
                        .send(Message::Pong(bytes))
                        .await
                        .map_err(|_| RelayOutputError::Unavailable)?,
                    Some(Ok(Message::Pong(_))) => {}
                    _ => return Err(RelayOutputError::Unavailable),
                }
            }
            Err(RelayOutputError::Invalid)
        })
        .await
        .map_err(|_| RelayOutputError::Unavailable)?
    }
}
