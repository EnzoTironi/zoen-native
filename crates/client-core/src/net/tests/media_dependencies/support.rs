//! Shared vault, owned directory and actual command-FIFO barrier.
use super::*;

#[derive(Default)]
pub(super) struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}
pub(super) struct Directory(pub(super) std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) async fn wire_barrier(
    socket: &mut RelaySocket,
    commands: &mpsc::UnboundedSender<Cmd>,
    name: &str,
) {
    // Flush and request share the actual command FIFO. Every resulting Publish
    // would be on this socket before the marker; no sleep infers its absence.
    commands.send(Cmd::Flush).unwrap();
    let (answer, answered) = oneshot::channel();
    commands
        .send(Cmd::Req {
            op: Op::FetchLink { id: name.into() },
            reply: answer,
        })
        .unwrap();
    loop {
        match request(socket).await {
            ClientFrame::Req {
                id,
                op: Op::FetchLink { id: marker },
            } if marker == name => {
                reply(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Link(None)),
                    },
                )
                .await;
                break;
            }
            ClientFrame::Req { id, .. } => {
                reply(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Done),
                    },
                )
                .await
            }
            ClientFrame::Ping => {
                reply(
                    socket,
                    ServerFrame::Pong {
                        server_time_ms: None,
                    },
                )
                .await
            }
            ClientFrame::Ephemeral { .. } => {}
            other => panic!("unexpected publication before {name} barrier: {other:?}"),
        }
    }
    assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
}
