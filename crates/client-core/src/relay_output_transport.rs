//! Bounded transport for a retained native publication. This uses the same
//! Hello/Auth, blob PUT and Publish protocol as Net, without owning any state.
use crate::blob_transport::{BlobUploadError, BlobUploadReceipt};
use event_log::Author;
use futures_util::{SinkExt, StreamExt};
use protocol::runtime_binding::{BlobBinding, Expectation, CAPABILITY, MANAGED_CAPABILITY};
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
    #[error("original managed relay output is uncertain")]
    Unknown,
    #[error(transparent)]
    Blob(#[from] BlobUploadError),
}

#[cfg(test)]
mod tests;

pub struct RelayOutputTransport {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    http: reqwest::Client,
    base: String,
    relay: String,
    actor: Author,
    binding_capable: bool,
    managed_capable: bool,
    binding: Option<BlobBinding>,
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
            binding_capable: false,
            managed_capable: false,
            binding: None,
        };
        tokio::time::timeout(
            Duration::from_secs(15),
            transport.authenticate(expected_relay),
        )
        .await
        .map_err(|_| RelayOutputError::Unavailable)??;
        Ok(transport)
    }

    /// The trusted Runtime derives the expectation from its private stores.
    /// This same socket is retained for every later publication; it never reconnects.
    pub async fn connect_bound(
        url: &str,
        expected_relay: &str,
        actor: Author,
        expected: Expectation,
    ) -> Result<Self, RelayOutputError> {
        let mut transport = Self::connect(url, expected_relay, actor).await?;
        transport.rebind_expected(expected).await?;
        Ok(transport)
    }

    async fn rebind_expected(&mut self, expected: Expectation) -> Result<(), RelayOutputError> {
        if !self.binding_capable || expected.request().managed.is_some() && !self.managed_capable {
            return Err(RelayOutputError::Refused);
        }
        self.binding = None;
        self.send(ClientFrame::Req {
            id: 1,
            op: protocol::Op::RuntimeBinding(expected.request().clone()),
        })
        .await?;
        tokio::time::timeout(Duration::from_secs(15), self.bind(expected))
            .await
            .map_err(|_| RelayOutputError::Unavailable)??;
        Ok(())
    }

    async fn bind(&mut self, expected: Expectation) -> Result<(), RelayOutputError> {
        for _ in 0..128 {
            match self.receive().await? {
                ServerFrame::Res {
                    id: 1,
                    result: Ok(protocol::Reply::RuntimeBinding(proof)),
                } => {
                    if !expected.verify(&proof) {
                        return Err(RelayOutputError::Refused);
                    }
                    self.binding = Some(BlobBinding {
                        request: expected.request().clone(),
                        proof,
                        identity: self.actor.identity.clone(),
                    });
                    return Ok(());
                }
                ServerFrame::Presence { .. }
                | ServerFrame::KeyPackagesLow { .. }
                | ServerFrame::ProfileChanged { .. }
                | ServerFrame::Ephemeral { .. } => {}
                _ => return Err(RelayOutputError::Refused),
            }
        }
        Err(RelayOutputError::Unavailable)
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
            capabilities,
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
                    self.binding_capable = capabilities.iter().any(|c| c == CAPABILITY);
                    self.managed_capable = capabilities.iter().any(|c| c == MANAGED_CAPABILITY);
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
        if let Some(binding) = &self.binding {
            return crate::blob_transport::put_bound(
                &self.http,
                crate::blob_transport::BoundUpload {
                    base: &self.base,
                    relay: &self.relay,
                    key: &self.actor.key,
                    binding,
                },
                sha,
                bytes,
                at_ms,
            )
            .await
            .map_err(Into::into);
        }
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
        if self
            .binding
            .as_ref()
            .is_some_and(|binding| binding.request.managed.is_some())
        {
            return Err(RelayOutputError::Refused);
        }
        self.publish_checked(env, false).await
    }

    /// Every managed effect supplies a fresh expectation derived by its trusted
    /// owner from the retained checkpoint. Rebind on the original socket; never
    /// reconnect or silently fall back to an ordinary Publish.
    pub async fn publish_managed(
        &mut self,
        env: &Envelope,
        expected: Expectation,
    ) -> Result<u64, RelayOutputError> {
        if expected.request().managed.is_none() {
            return Err(RelayOutputError::Invalid);
        }
        self.rebind_expected(expected).await?;
        self.publish_checked(env, true).await
    }

    async fn publish_checked(
        &mut self,
        env: &Envelope,
        managed: bool,
    ) -> Result<u64, RelayOutputError> {
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
        tokio::time::timeout(Duration::from_secs(15), self.acceptance(env, managed))
            .await
            .map_err(|_| RelayOutputError::Unavailable)?
    }

    async fn acceptance(&mut self, env: &Envelope, managed: bool) -> Result<u64, RelayOutputError> {
        for _ in 0..128 {
            match self.receive().await? {
                ServerFrame::Accepted {
                    space,
                    client_id,
                    seq,
                } if space == env.space() && client_id == env.client_id() => return Ok(seq),
                ServerFrame::Rejected {
                    space,
                    client_id,
                    permanent,
                    ..
                } if space == env.space() && client_id == env.client_id() => {
                    if managed && !permanent {
                        return Err(RelayOutputError::Unknown);
                    }
                    return Err(RelayOutputError::Refused);
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
