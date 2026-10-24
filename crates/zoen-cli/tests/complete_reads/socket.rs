//! A second actual certified socket holds the read while public clients mutate.
use super::*;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    complete_reads::{Assembly, Kind, Page, Request},
    ClientFrame, Op, Reply, ServerFrame,
};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
pub(super) struct Reader {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    identity: String,
    device: String,
}
impl Reader {
    pub(super) async fn open(w: &World, who: &str, account: client_core::AccountDto) -> Self {
        // The live Client owns SQLite exclusively. This independent authenticated
        // socket shares its already-read public account, never a second DB handle.
        let home = w.dir.join(who);
        let key: [u8; 32] = std::fs::read(home.join("vault/zoen.device.v1"))
            .unwrap()
            .try_into()
            .unwrap();
        let signer = event_log::Signer::from_secret(&key);
        assert_eq!(signer.id(), account.device_id);
        let (socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/v1/sync", w.port))
                .await
                .unwrap();
        let mut reader = Self {
            socket,
            identity: account.identity_id,
            device: account.device_id,
        };
        reader
            .send(ClientFrame::Hello {
                protocol: protocol::PROTOCOL_VERSION,
                capabilities: vec![protocol::complete_reads::CAPABILITY.into()],
                identity: reader.identity.clone(),
                device: reader.device.clone(),
                cert: {
                    let root: [u8; 32] = std::fs::read(home.join("vault/zoen.identity.v1"))
                        .unwrap()
                        .try_into()
                        .unwrap();
                    event_log::Signer::from_secret(&root)
                        .sign(&event_log::device_cert_message(&reader.device))
                },
            })
            .await;
        let ServerFrame::Challenge {
            nonce,
            relay,
            capabilities,
            ..
        } = reader.recv().await
        else {
            panic!("challenge")
        };
        assert!(capabilities
            .iter()
            .any(|s| s == protocol::complete_reads::CAPABILITY));
        reader
            .send(ClientFrame::Auth {
                sig: signer.sign(&protocol::auth_message(&nonce, &relay)),
            })
            .await;
        assert!(matches!(
            reader.recv().await,
            ServerFrame::Ready {
                registered: true,
                ..
            }
        ));
        reader
    }
    async fn send(&mut self, frame: ClientFrame) {
        self.socket
            .send(Message::Binary(frame.encode().into()))
            .await
            .unwrap();
    }
    async fn recv(&mut self) -> ServerFrame {
        loop {
            let message = tokio::time::timeout(Duration::from_secs(5), self.socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            if let Message::Binary(bytes) = message {
                return ServerFrame::decode(&bytes).unwrap();
            }
        }
    }
    async fn page(&mut self, request: Request) -> Page {
        self.send(ClientFrame::Req {
            id: 992,
            op: Op::CompleteRead(request),
        })
        .await;
        loop {
            if let ServerFrame::Res { id: 992, result } = self.recv().await {
                let Reply::CompletePage(page) = result.unwrap() else {
                    panic!("expected complete page")
                };
                assert!(page.valid());
                return page;
            }
        }
    }
    pub(super) async fn begin(&mut self) -> (Page, Assembly) {
        let page = self.page(Request::begin(Kind::AgentState)).await;
        let assembly = Assembly::new(Kind::AgentState, self.identity.clone(), self.device.clone());
        (page, assembly)
    }
    pub(super) async fn finish(
        &mut self,
        first: Page,
        assembly: &mut Assembly,
    ) -> protocol::AgentStateRecord {
        let mut next = assembly.accept(first).unwrap();
        while let Some(request) = next {
            next = assembly.accept(self.page(request).await).unwrap();
        }
        // Finish consumes the staged set; replace only this test's local assembler.
        let owned = std::mem::replace(
            assembly,
            Assembly::new(Kind::AgentState, self.identity.clone(), self.device.clone()),
        );
        let Reply::AgentState(state) = owned.finish().unwrap() else {
            panic!("state")
        };
        state
    }
}
