use super::*;
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientFrame, ServerFrame, PROTOCOL_VERSION};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

const BASE: &str = r#"{"version":1,"kind":"user","persona":"Enrollment validation only.","rules":["Ask before effects."],"state":{},"model_tier":"cheap"}"#;
fn record(version: u32, bytes: &str) -> Record {
    Record {
        status: "active".into(),
        kind: "User".into(),
        version,
        bytes: bytes.into(),
    }
}
fn scope() -> Scope {
    Scope {
        owner: "a".repeat(64),
        device: "b".repeat(64),
        certificate_sha256: "c".repeat(64),
        relay: "https://relay.example".into(),
        configured_relay: "https://relay.example".into(),
        agent: "d".repeat(64),
    }
}
fn edit() -> DefinitionEdit {
    DefinitionEdit {
        version: 1,
        sha256: sha(BASE.as_bytes()),
        persona: "Help with reviewed presentation tasks.".into(),
        rules: vec!["Respect owner review.".into()],
    }
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let p = std::env::temp_dir().join(types::new_id("zoen-definition-test")?);
        std::fs::create_dir(&p).unwrap();
        Ok(Self(p))
    }
    fn path(&self) -> String {
        self.0.join("owner.sqlite").to_string_lossy().into()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Call {
    op: AgentRequest,
    response: oneshot::Sender<Result<Reply, String>>,
}
impl Call {
    fn status(self, value: &Record) {
        assert!(matches!(self.op, AgentRequest::Status { .. }));
        self.response
            .send(Ok(Reply::AgentState(AgentStateRecord {
                status: value.status.clone(),
                kind: value.kind.clone(),
                definition_version: value.version,
                definition: value.bytes.as_bytes().to_vec(),
                cursors: vec![],
            })))
            .unwrap();
    }
}
async fn call(rx: &mut mpsc::UnboundedReceiver<Call>) -> Call {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap()
}
struct Relay {
    url: String,
    calls: mpsc::UnboundedReceiver<Call>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn relay() -> Relay {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, calls) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let Some(Ok(Message::Binary(bytes))) = ws.next().await else {
                    return;
                };
                let ClientFrame::Hello {
                    identity,
                    device,
                    cert,
                    ..
                } = ClientFrame::decode(&bytes).unwrap()
                else {
                    panic!("Hello");
                };
                assert!(event_log::verify_sig(
                    &identity,
                    &event_log::device_cert_message(&device),
                    &cert
                ));
                ws.send(Message::Binary(
                    ServerFrame::Challenge {
                        nonce: "definition-test".into(),
                        relay: "definition-test".into(),
                        protocol: PROTOCOL_VERSION,
                        capabilities: vec![],
                        server_time_ms: None,
                    }
                    .encode()
                    .into(),
                ))
                .await
                .unwrap();
                let Some(Ok(Message::Binary(bytes))) = ws.next().await else {
                    return;
                };
                let ClientFrame::Auth { sig } = ClientFrame::decode(&bytes).unwrap() else {
                    panic!("Auth");
                };
                assert!(event_log::verify_sig(
                    &device,
                    &protocol::auth_message("definition-test", "definition-test"),
                    &sig
                ));
                ws.send(Message::Binary(
                    ServerFrame::Ready {
                        identity,
                        registered: true,
                    }
                    .encode()
                    .into(),
                ))
                .await
                .unwrap();
                while let Some(Ok(Message::Binary(bytes))) = ws.next().await {
                    let response = match ClientFrame::decode(&bytes).unwrap() {
                        ClientFrame::Req {
                            id,
                            op: Op::Agent(op),
                        } => {
                            let (response, rx) = oneshot::channel();
                            tx.send(Call { op, response }).unwrap();
                            ServerFrame::Res {
                                id,
                                result: rx.await.unwrap(),
                            }
                        }
                        ClientFrame::Req { id, .. } => ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                        ClientFrame::Sync { .. } => ServerFrame::SyncDone,
                        ClientFrame::Ping => ServerFrame::Pong {
                            server_time_ms: None,
                        },
                        _ => continue,
                    };
                    if ws
                        .send(Message::Binary(response.encode().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
    });
    Relay { url, calls, task }
}
fn owner(path: String, relay: &str) -> (Arc<Client>, [u8; 32]) {
    let c = Client::open(path, "en".into()).unwrap();
    let (_, device, _) = c
        .lock()
        .create_account("Synthetic editor owner", "definition_test", relay)
        .unwrap();
    c.lock().set_registered(true).unwrap(); // Offline test server, not real relay acceptance.
    (c, device)
}
fn owner_with_configured_relay(path: String, relay: &str) -> (Arc<Client>, [u8; 32]) {
    let (c, key) = owner(path, relay);
    // Legacy account creation strips trailing slashes. The existing linked
    // account writer preserves the exact configured value exposed by AccountDto.
    let mut account = c.lock().account().unwrap().clone();
    account.relay_url = relay.into();
    c.lock().save_linked_account(account).unwrap();
    assert_eq!(c.account().unwrap().relay_url, relay);
    (c, key)
}
fn target(c: &Client) -> DefinitionTarget {
    let a = c.account().unwrap();
    DefinitionTarget {
        agent: event_log::Signer::from_secret(&[7; 32]).id(),
        identity: a.identity_id,
        device: a.device_id,
    }
}
fn command(c: &Client, op: &str) -> serde_json::Value {
    let t = target(c);
    serde_json::json!({"op":op,"agent_id":t.agent,"expected_identity_id":t.identity,"expected_device_id":t.device})
}
async fn online(c: &Client) {
    c.start_sync(None).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while c.connection().state != "online" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn preview(c: &Arc<Client>, r: &mut Relay) -> AgentDefinitionPreviewDto {
    preview_record(c, r, &record(1, BASE)).await
}
async fn preview_record(
    c: &Arc<Client>,
    r: &mut Relay,
    before: &Record,
) -> AgentDefinitionPreviewDto {
    let c2 = c.clone();
    let mut input = command(c, "agent_definition_preview");
    input["observed_version"] = 1.into();
    input["observed_definition_sha256"] = sha(BASE.as_bytes()).into();
    input["persona"] = edit().persona.into();
    input["rules"] = serde_json::json!(edit().rules);
    let task = tokio::spawn(async move { c2.client_request(input.to_string()).await });
    call(&mut r.calls).await.status(before);
    serde_json::from_str(&task.await.unwrap().unwrap()).unwrap()
}
fn update(c: &Arc<Client>, p: &AgentDefinitionPreviewDto) -> tokio::task::JoinHandle<R<String>> {
    let c2 = c.clone();
    let mut input = command(c, "agent_definition_update");
    input["observed_version"] = p.current.version.into();
    input["observed_definition_sha256"] = p.current.definition_sha256.clone().into();
    input["persona"] = p.proposed.persona.clone().into();
    input["rules"] = serde_json::json!(p.proposed.rules);
    input["observed_preview_digest"] = p.preview_digest.clone().into();
    tokio::spawn(async move { c2.client_request(input.to_string()).await })
}
fn response(task: String) -> AgentDefinitionUpdateDto {
    serde_json::from_str(&task).unwrap()
}

#[test]
fn configured_relay_dto_preserves_accepted_case_default_port_and_transport_spellings() {
    for (raw, canonical) in [
        ("https://RELAY.example:443/", "https://relay.example"),
        ("https://relay.example:443////", "https://relay.example"),
        ("wss://RELAY.example:443/", "https://relay.example"),
        ("ws://LOCALHOST:80/", "http://localhost"),
        ("relay.example:443/", "https://relay.example"),
    ] {
        let dir = Directory::new().unwrap();
        let (c, _) = owner_with_configured_relay(dir.path(), raw);
        let t = target(&c);
        let captured = Scope::capture(&c.lock(), &t).unwrap();
        assert_eq!(captured.relay, canonical);
        assert_eq!(captured.configured_relay, raw);
        let review = Receipt::prepare(captured, record(1, BASE), edit())
            .unwrap()
            .review()
            .unwrap();
        assert_eq!(review.current.relay_url, raw);
        assert_eq!(review.proposed.relay_url, raw);
        assert_eq!(c.account().unwrap().relay_url, raw);
    }
}

#[test]
fn original_authority_key_is_byte_exact_and_raw_configuration_is_in_review_binding() {
    let original = scope();
    let legacy = format!("{{\"owner\":\"{}\",\"device\":\"{}\",\"certificate_sha256\":\"{}\",\"relay\":\"{}\",\"agent\":\"{}\"}}", original.owner, original.device, original.certificate_sha256, original.relay, original.agent);
    assert_eq!(
        original.key().unwrap(),
        format!(
            "agent.definition.owner-update.v1:{}",
            sha(legacy.as_bytes())
        )
    );
    let mut raw_changed = original.clone();
    raw_changed.configured_relay.push('/');
    assert_eq!(original.key().unwrap(), raw_changed.key().unwrap());
    let before = Receipt::prepare(original, record(1, BASE), edit()).unwrap();
    let after = Receipt::prepare(raw_changed, record(1, BASE), edit()).unwrap();
    assert_ne!(before.preview_digest, after.preview_digest);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_loopback_read_update_and_cold_reconcile_keep_exact_configured_relay() {
    for spelling in ["slash", "websocket", "host-case"] {
        let dir = Directory::new().unwrap();
        let mut r = relay().await;
        let raw = match spelling {
            "slash" => format!("{}/", r.url),
            "websocket" => format!("{}/", r.url.replacen("http://", "ws://", 1)),
            _ => format!("{}/", r.url.replace("127.0.0.1", "LOCALHOST")),
        };
        let (c, key) = owner_with_configured_relay(dir.path(), &raw);
        online(&c).await;
        let c2 = c.clone();
        let input = command(&c, "agent_definition_read");
        let task = tokio::spawn(async move { c2.client_request(input.to_string()).await });
        call(&mut r.calls).await.status(&record(1, BASE));
        let read: AgentDefinitionDto = serde_json::from_str(&task.await.unwrap().unwrap()).unwrap();
        assert_eq!(read.relay_url, raw);
        let p = preview(&c, &mut r).await;
        assert_eq!(p.current.relay_url, raw);
        let task = update(&c, &p);
        call(&mut r.calls).await.status(&record(1, BASE));
        let define = call(&mut r.calls).await;
        assert!(matches!(define.op, AgentRequest::Define { version: 2, .. }));
        let t = target(&c);
        let scope = Scope::capture(&c.lock(), &t).unwrap();
        let held = Receipt::load(&c.lock(), &scope).unwrap().unwrap();
        define
            .response
            .send(Err("synthetic lost reply after accepted update".into()))
            .unwrap();
        call(&mut r.calls).await.status(&held.after);
        let result = response(task.await.unwrap().unwrap());
        assert_eq!(result.outcome, AgentDefinitionUpdateOutcome::Confirmed);
        assert_eq!(result.current.unwrap().relay_url, raw);
        c.stop_sync();
        drop(c);
        let cold = Client::open(dir.path(), "en".into()).unwrap();
        assert!(cold.lock().unlock(Some(key.to_vec())).unwrap());
        online(&cold).await;
        let c2 = cold.clone();
        let input = command(&cold, "agent_definition_reconcile");
        let task = tokio::spawn(async move { c2.client_request(input.to_string()).await });
        call(&mut r.calls).await.status(&held.after); // Read only; never another Define.
        let result = response(task.await.unwrap().unwrap());
        assert_eq!(result.outcome, AgentDefinitionUpdateOutcome::Confirmed);
        assert_eq!(result.review.current.relay_url, raw);
        assert_eq!(result.current.unwrap().relay_url, raw);
        cold.stop_sync();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_only_configuration_change_retires_inflight_read_and_same_key_receipt() {
    let dir = Directory::new().unwrap();
    let mut r = relay().await;
    let raw = format!("{}/", r.url);
    let (c, _) = owner_with_configured_relay(dir.path(), &raw);
    online(&c).await;
    let t = target(&c);
    let original = Scope::capture(&c.lock(), &t).unwrap();
    let held = Receipt::prepare(original.clone(), record(1, BASE), edit()).unwrap();
    c.lock()
        .store
        .set_meta(
            &original.key().unwrap(),
            &serde_json::to_string(&held).unwrap(),
        )
        .unwrap();
    let c2 = c.clone();
    let input = command(&c, "agent_definition_read");
    let task = tokio::spawn(async move { c2.client_request(input.to_string()).await });
    let request = call(&mut r.calls).await;
    {
        let mut engine = c.lock();
        let mut account = engine.account().unwrap().clone();
        account.relay_url = r.url.clone();
        engine.save_linked_account(account).unwrap();
    }
    request.status(&record(1, BASE));
    assert!(task.await.unwrap().is_err());
    let t = target(&c);
    let changed = Scope::capture(&c.lock(), &t).unwrap();
    assert_eq!(original.relay, changed.relay);
    assert_ne!(original.configured_relay, changed.configured_relay);
    assert_eq!(original.key().unwrap(), changed.key().unwrap());
    assert!(c.agent_definition_reconcile(target(&c)).await.is_err());
    assert!(update(&c, &held.review().unwrap()).await.unwrap().is_err());
    assert!(r.calls.try_recv().is_err());
    assert!(c
        .lock()
        .store
        .meta(&original.key().unwrap())
        .unwrap()
        .is_some());
    c.stop_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn legacy_v306_same_key_receipt_fails_closed_before_any_new_dispatch() {
    let dir = Directory::new().unwrap();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let t = target(&c);
    let scope = Scope::capture(&c.lock(), &t).unwrap();
    let held = Receipt::prepare(scope.clone(), record(1, BASE), edit()).unwrap();
    let review = held.review().unwrap();
    let mut legacy = serde_json::to_value(&held).unwrap();
    legacy["scope"]
        .as_object_mut()
        .unwrap()
        .remove("configured_relay");
    let bytes = serde_json::to_string(&legacy).unwrap();
    c.lock()
        .store
        .set_meta(&scope.key().unwrap(), &bytes)
        .unwrap();
    assert!(c.agent_definition_reconcile(target(&c)).await.is_err());
    assert!(update(&c, &review).await.unwrap().is_err());
    assert!(r.calls.try_recv().is_err());
    assert_eq!(
        c.lock()
            .store
            .meta(&scope.key().unwrap())
            .unwrap()
            .as_deref(),
        Some(bytes.as_str())
    );
    c.stop_sync();
}

mod schema_cases;

mod reconcile_cases;

#[path = "../agent_control/tests.rs"]
mod control_cases;
