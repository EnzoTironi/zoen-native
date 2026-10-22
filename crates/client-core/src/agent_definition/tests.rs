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
#[test]
fn strict_shared_schema_and_metadata_refuse_substitution() {
    assert!(UserDefinition::parse_bootstrap(BASE).is_ok());
    let accepted = UserDefinition::parse(BASE, 1)
        .unwrap()
        .edited(edit().persona, edit().rules)
        .unwrap();
    assert!(UserDefinition::parse_bootstrap(&accepted).is_err());
    assert!(UserDefinition::parse(&accepted, 2).is_ok());
    for source in [
        BASE.replace("\"kind\":\"user\"", "\"kind\":\"creator\""),
        BASE.replace("\"cheap\"", "\"premium\""),
        BASE.replace("\"state\":{}", "\"state\":{\"token\":\"no\"}"),
        BASE.replace("\"version\":1", "\"version\":1,\"version\":1"),
        BASE.replace("\"state\":{},", ""),
        BASE.replace("\"kind\":", "\"unknown\":0,\"kind\":"),
        format!(" {BASE}"),
        BASE.replace("\"version\":1", "\"version\":0"),
    ] {
        assert!(UserDefinition::parse(&source, 1).is_err());
    }
    let mut value = record(1, BASE);
    value.version = 2;
    assert!(value.dto(&scope()).is_err());
    value.version = 1;
    value.kind = "Creator".into();
    assert!(value.dto(&scope()).is_err());
    assert!(UserDefinition::parse(BASE, 0).is_err());
    assert!(UserDefinition::parse(BASE, u32::MAX).is_err());
    let max = BASE.replacen("\"version\":1", &format!("\"version\":{}", i32::MAX), 1);
    assert!(UserDefinition::parse(&max, i32::MAX as u32)
        .unwrap()
        .edited(edit().persona, edit().rules)
        .is_err());
    assert!(Record::from_relay(AgentStateRecord {
        status: "active".into(),
        kind: "User".into(),
        definition_version: 1,
        definition: vec![],
        cursors: vec![]
    })
    .is_err());
}
#[test]
fn review_digest_binds_full_scope_before_after_and_status() {
    let receipt = Receipt::prepare(scope(), record(1, BASE), edit()).unwrap();
    let review = receipt.review().unwrap();
    assert_eq!(review.proposed.version, 2);
    assert_ne!(
        review.current.definition_sha256,
        review.proposed.definition_sha256
    );
    for field in [
        "owner",
        "device",
        "certificate_sha256",
        "relay",
        "agent",
        "configured_relay",
    ] {
        let mut encoded = serde_json::to_value(&receipt).unwrap();
        encoded["scope"][field] = "changed".into();
        assert!(serde_json::from_value::<Receipt>(encoded)
            .unwrap()
            .review()
            .is_err());
    }
    let mut paused = receipt.clone();
    paused.before.status = "paused".into();
    assert!(paused.review().is_err());
    let mut wrong = edit();
    wrong.sha256 = "0".repeat(64);
    assert!(Receipt::prepare(scope(), record(1, BASE), wrong).is_err());
}
#[test]
fn old_base_is_nonterminal_unknown_and_exact_next_bytes_are_required() {
    let receipt = Receipt::prepare(scope(), record(1, BASE), edit()).unwrap();
    assert_eq!(
        receipt
            .outcome(Some(receipt.before.clone()))
            .unwrap()
            .outcome,
        AgentDefinitionUpdateOutcome::Unknown
    );
    assert_eq!(
        receipt.outcome(None).unwrap().outcome,
        AgentDefinitionUpdateOutcome::Unknown
    );
    assert_eq!(
        receipt
            .outcome(Some(receipt.after.clone()))
            .unwrap()
            .outcome,
        AgentDefinitionUpdateOutcome::Confirmed
    );
    let mut different = receipt.after.clone();
    different.bytes = different.bytes.replace("Help with", "Avoid");
    assert_eq!(
        receipt.outcome(Some(different)).unwrap().outcome,
        AgentDefinitionUpdateOutcome::Conflict
    );
    let mut newer = receipt.after.clone();
    newer.version = 3;
    assert_eq!(
        receipt.outcome(Some(newer)).unwrap().outcome,
        AgentDefinitionUpdateOutcome::Conflict
    );
    let mut revoked = receipt.after.clone();
    revoked.status = "revoked".into();
    assert_eq!(
        receipt.outcome(Some(revoked)).unwrap().outcome,
        AgentDefinitionUpdateOutcome::Conflict
    );
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(types::new_id("zoen-definition-test"));
        std::fs::create_dir(&p).unwrap();
        Self(p)
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_update_persists_before_dispatch_reconciles_lost_ack_and_never_resends() {
    let dir = Directory::new();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let p = preview(&c, &mut r).await;
    let task = update(&c, &p);
    call(&mut r.calls).await.status(&record(1, BASE));
    let define = call(&mut r.calls).await;
    let own_target = target(&c);
    let scope = Scope::capture(&c.lock(), &own_target).unwrap();
    let held = Receipt::load(&c.lock(), &scope).unwrap().unwrap();
    let AgentRequest::Define {
        version,
        definition,
        ..
    } = &define.op
    else {
        panic!("Define");
    };
    assert_eq!(*version, 2);
    assert_eq!(*definition, held.after.bytes.as_bytes());
    define
        .response
        .send(Err("synthetic lost acknowledgement after acceptance".into()))
        .unwrap();
    call(&mut r.calls).await.status(&held.after);
    assert_eq!(
        response(task.await.unwrap().unwrap()).outcome,
        AgentDefinitionUpdateOutcome::Confirmed
    );
    let duplicate = update(&c, &p);
    call(&mut r.calls).await.status(&held.after); // No second Define is allowed.
    assert_eq!(
        response(duplicate.await.unwrap().unwrap()).receipt_id,
        held.id
    );
    assert!(!c
        .agent_definition_dismiss(target(&c), "old-receipt", &p.preview_digest)
        .unwrap());
    assert!(!c
        .agent_definition_dismiss(target(&c), &held.id, &"0".repeat(64))
        .unwrap());
    assert!(c
        .agent_definition_dismiss(target(&c), &held.id, &p.preview_digest)
        .unwrap());
    assert!(c
        .agent_definition_reconcile(target(&c))
        .await
        .unwrap()
        .is_none());
    c.stop_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn old_base_after_unknown_stays_held_across_cold_reopen_until_late_exact_commit() {
    let dir = Directory::new();
    let mut r = relay().await;
    let (c, key) = owner(dir.path(), &r.url);
    online(&c).await;
    let p = preview(&c, &mut r).await;
    let task = update(&c, &p);
    call(&mut r.calls).await.status(&record(1, BASE));
    let define = call(&mut r.calls).await;
    define
        .response
        .send(Err("synthetic reply lost, request may arrive later".into()))
        .unwrap();
    let own_target = target(&c);
    let own_scope = Scope::capture(&c.lock(), &own_target).unwrap();
    let held = Receipt::load(&c.lock(), &own_scope).unwrap().unwrap();
    call(&mut r.calls).await.status(&held.before);
    assert_eq!(
        response(task.await.unwrap().unwrap()).outcome,
        AgentDefinitionUpdateOutcome::Unknown
    );
    c.stop_sync();
    drop(c);
    let cold = Client::open(dir.path(), "en".into()).unwrap();
    assert!(cold.lock().unlock(Some(key.to_vec())).unwrap());
    let offline = cold
        .agent_definition_reconcile(target(&cold))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offline.outcome, AgentDefinitionUpdateOutcome::Unknown);
    assert_eq!(offline.receipt_id, held.id);
    online(&cold).await;
    let c2 = cold.clone();
    let input = command(&cold, "agent_definition_reconcile");
    let task = tokio::spawn(async move { c2.client_request(input.to_string()).await });
    call(&mut r.calls).await.status(&held.after);
    assert_eq!(
        response(task.await.unwrap().unwrap()).outcome,
        AgentDefinitionUpdateOutcome::Confirmed
    );
    cold.stop_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_forward_versions_conflict_without_a_version_bump() {
    let dir = Directory::new();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    for concurrent in [2, 3] {
        let p = preview(&c, &mut r).await;
        let task = update(&c, &p);
        call(&mut r.calls).await.status(&record(1, BASE));
        let define = call(&mut r.calls).await;
        assert!(matches!(define.op, AgentRequest::Define { version: 2, .. }));
        define
            .response
            .send(Err("version must increase".into()))
            .unwrap();
        let bytes = BASE
            .replace("\"version\":1", &format!("\"version\":{concurrent}"))
            .replace("Enrollment validation only.", "Concurrent owner change.");
        call(&mut r.calls).await.status(&record(concurrent, &bytes));
        let result = response(task.await.unwrap().unwrap());
        assert_eq!(result.outcome, AgentDefinitionUpdateOutcome::Conflict);
        assert_eq!(result.current.unwrap().version, concurrent);
        assert!(c
            .agent_definition_dismiss(target(&c), &result.receipt_id, &p.preview_digest)
            .unwrap());
    }
    c.stop_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn old_relay_missing_bytes_and_device_change_fail_closed() {
    let dir = Directory::new();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let input = command(&c, "agent_definition_read");
    let c2 = c.clone();
    let read = tokio::spawn(async move { c2.client_request(input.to_string()).await });
    call(&mut r.calls).await.status(&record(1, ""));
    assert!(read.await.unwrap().is_err());
    let input = command(&c, "agent_definition_read");
    let c2 = c.clone();
    let read = tokio::spawn(async move { c2.client_request(input.to_string()).await });
    let request = call(&mut r.calls).await;
    c.lock().net.author = None;
    request.status(&record(1, BASE));
    assert!(read.await.unwrap().is_err());
    c.stop_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_sqlite_receipt_write_refuses_dispatch() {
    let dir = Directory::new();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let p = preview(&c, &mut r).await;
    for trigger in [
        "CREATE TEMP TRIGGER refuse_definition_receipt BEFORE INSERT ON meta WHEN NEW.key LIKE 'agent.definition.owner-update.v1:%' BEGIN SELECT RAISE(ABORT, 'synthetic receipt write failure'); END;",
        "CREATE TEMP TRIGGER refuse_definition_receipt AFTER INSERT ON meta WHEN NEW.key LIKE 'agent.definition.owner-update.v1:%' BEGIN DELETE FROM meta WHERE key=NEW.key; END;",
    ] {
        c.lock().store.conn().execute_batch(trigger).unwrap();
        let task = update(&c, &p);
        call(&mut r.calls).await.status(&record(1, BASE));
        assert!(task.await.unwrap().is_err());
        assert!(r.calls.try_recv().is_err()); // No Define without retained/read-back review.
        let t = target(&c);
        let scope = Scope::capture(&c.lock(), &t).unwrap();
        assert!(Receipt::load(&c.lock(), &scope).unwrap().is_none());
        c.lock().store.conn().execute_batch("DROP TRIGGER refuse_definition_receipt;").unwrap();
    }
    c.stop_sync();
}

#[test]
fn dismiss_cas_preserves_newer_receipt_and_foreign_device_cannot_restore_it() {
    let dir = Directory::new();
    let (c, _) = owner(dir.path(), "https://relay.example");
    let t = target(&c);
    let scope = Scope::capture(&c.lock(), &t).unwrap();
    let old = Receipt::prepare(scope.clone(), record(1, BASE), edit()).unwrap();
    let newer = Receipt::prepare(scope.clone(), record(1, BASE), edit()).unwrap();
    assert_ne!(old.id, newer.id);
    c.lock()
        .store
        .set_meta(
            &scope.key().unwrap(),
            &serde_json::to_string(&newer).unwrap(),
        )
        .unwrap();
    assert!(!c
        .agent_definition_dismiss(target(&c), &old.id, &old.preview_digest)
        .unwrap());
    assert_eq!(
        Receipt::load(&c.lock(), &scope).unwrap().unwrap().id,
        newer.id
    );
    let mut foreign = target(&c);
    foreign.device = event_log::Signer::from_secret(&[8; 32]).id();
    assert!(c
        .agent_definition_dismiss(foreign, &newer.id, &newer.preview_digest)
        .is_err());
    assert_eq!(
        Receipt::load(&c.lock(), &scope).unwrap().unwrap().id,
        newer.id
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paused_definition_edit_preserves_status_and_malformed_receipt_never_dispatches() {
    let dir = Directory::new();
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let mut paused = record(1, BASE);
    paused.status = "paused".into();
    let p = preview_record(&c, &mut r, &paused).await;
    assert_eq!(p.current.status, AgentStatus::Paused);
    assert_eq!(p.proposed.status, AgentStatus::Paused);
    let task = update(&c, &p);
    call(&mut r.calls).await.status(&paused);
    let define = call(&mut r.calls).await;
    let t = target(&c);
    let scope = Scope::capture(&c.lock(), &t).unwrap();
    let receipt = Receipt::load(&c.lock(), &scope).unwrap().unwrap();
    assert!(matches!(define.op, AgentRequest::Define { version: 2, .. }));
    define.response.send(Ok(Reply::Done)).unwrap();
    call(&mut r.calls).await.status(&receipt.after);
    let result = response(task.await.unwrap().unwrap());
    assert_eq!(result.outcome, AgentDefinitionUpdateOutcome::Confirmed);
    assert_eq!(result.current.unwrap().status, AgentStatus::Paused);
    c.lock()
        .store
        .set_meta(&scope.key().unwrap(), "malformed retained recovery context")
        .unwrap();
    assert!(update(&c, &p).await.unwrap().is_err());
    assert!(r.calls.try_recv().is_err()); // Neither Status nor Define after load error.
    assert_eq!(
        c.lock().store.meta(&scope.key().unwrap()).unwrap().unwrap(),
        "malformed retained recovery context"
    );
    c.stop_sync();
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
        let dir = Directory::new();
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
        let dir = Directory::new();
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
    let dir = Directory::new();
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
    let dir = Directory::new();
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
