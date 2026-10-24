use super::*;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

/// Actual private filesystem IO, atomic replacement and sync. Faults occur around
/// real writes/readbacks, never by replacing crypto/account validation with success.
struct DiskVault {
    dir: PathBuf,
    writes: AtomicUsize,
    crash_after: AtomicUsize,
    false_after: AtomicUsize,
    refuse: Mutex<Option<String>>,
    hide_read: AtomicBool,
}
impl DiskVault {
    fn new() -> std::result::Result<Arc<Self>, Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(types::new_id("zoen-enrollment-test")?);
        fs::create_dir(&dir).unwrap();
        Ok(Arc::new(Self {
            dir,
            writes: AtomicUsize::new(0),
            crash_after: AtomicUsize::new(0),
            false_after: AtomicUsize::new(0),
            refuse: Mutex::new(None),
            hide_read: AtomicBool::new(false),
        }))
    }
    fn vault(self: &Arc<Self>) -> Arc<dyn SecretVault> {
        self.clone()
    }
    fn db(&self) -> String {
        self.dir.join("host.sqlite").to_string_lossy().into()
    }
    fn client(&self) -> Arc<Client> {
        Client::open(self.db(), "en".into()).unwrap()
    }
}
impl Drop for DiskVault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
impl SecretVault for DiskVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        if self.hide_read.load(Ordering::SeqCst) {
            return None;
        }
        fs::read(self.dir.join(key)).ok()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        if self.refuse.lock().unwrap().as_deref() == Some(key.as_str()) {
            return false;
        }
        let temp = self.dir.join("private-vault-tmp");
        let result = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&value)?;
            file.sync_all()?;
            fs::rename(&temp, self.dir.join(&key))?;
            fs::File::open(&self.dir)?.sync_all()
        })();
        if result.is_err() {
            return false;
        }
        let write = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
        if self.crash_after.load(Ordering::SeqCst) == write {
            panic!("synthetic process cut after durable vault write");
        }
        self.false_after.load(Ordering::SeqCst) != write
    }
    fn delete(&self, key: String) {
        let _ = fs::remove_file(self.dir.join(key));
    }
}
fn make_owner() -> Arc<Client> {
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    client
        .lock()
        .create_account("Synthetic owner", "fixture_owner", "https://relay.example")
        .unwrap();
    // Offline fixture only: this does not prove relay registration/admission.
    client.lock().set_registered(true).unwrap();
    client
}
fn request(owner: &Client) -> AgentEnrollmentHostRequest {
    AgentEnrollmentHostRequest { expected_owner_id: owner.account().unwrap().identity_id,
        relay_url: "https://relay.example".into(), name: "Local helper".into(), handle: "local_helper".into(),
        definition_json: r#"{"version":1,"kind":"user","persona":"Help with reviewed tasks.","rules":["Ask before effects."],"state":{},"model_tier":"cheap"}"#.into() }
}
fn prepare(owner: &Client, host: &Client, vault: &Arc<DiskVault>) -> AgentEnrollmentHostOffer {
    host.prepare_agent_enrollment(request(owner), vault.vault())
        .unwrap()
}
fn make_envelope(owner: &Client, pending: &Pending) -> OwnerEnvelope {
    let account = owner.account().unwrap();
    let context = owner_context(&owner.lock(), &account.identity_id, &account.device_id).unwrap();
    let digest = pending.code.offer.digest().unwrap();
    let (proof, signature) =
        agent_owner::authorize_enrollment(&context.author, &pending.code.offer.agent_id, &digest)
            .unwrap();
    OwnerEnvelope {
        schema: 1,
        offer_digest: digest,
        owner_id: account.identity_id,
        agent_id: pending.code.offer.agent_id.clone(),
        owner_proof: proof,
        owner_offer_signature: signature,
    }
}
fn sealed(pending: &Pending, envelope: &OwnerEnvelope) -> Vec<u8> {
    crate::link::seal(
        &hex::decode(&pending.code.offer.hpke_public).unwrap(),
        &canonical_secret(&pending.code.pairing_secret).unwrap(),
        &pending.code.box_context().unwrap(),
        &serde_json::to_vec(envelope).unwrap(),
    )
    .unwrap()
}
fn accept(owner: &Client, vault: &Arc<DiskVault>) -> Pending {
    let mut pending = Pending::load(&vault.vault()).unwrap().unwrap();
    let public = make_envelope(owner, &pending);
    pending.accepted = Some(pending.open(&sealed(&pending, &public)).unwrap());
    pending.save(&vault.vault()).unwrap();
    pending
}

#[test]
fn agent_code_roundtrip_pins_owner_relay_profile_definition_and_rejects_device_link() {
    let owner = make_owner();
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    let offer = prepare(&owner, &host, &vault);
    let account = owner.account().unwrap();
    let preview = owner
        .agent_enrollment_preview(&offer.code, &account.identity_id, &account.device_id)
        .unwrap();
    assert_eq!(preview.offer_digest, offer.offer_digest);
    assert_eq!(preview.pairing_check, offer.pairing_check);
    assert_eq!(preview.name, "Local helper");
    assert!(host.account().is_none());
    assert!(vault.load(VAULT_DEVICE.into()).is_none());
    assert!(owner
        .agent_enrollment_status(&account.identity_id, &account.device_id)
        .unwrap()
        .is_none());
    assert_eq!(prepare(&owner, &host, &vault).code, offer.code);
    let parsed = Code::parse(&offer.code, now()).unwrap();
    for field in [
        "agent_id",
        "expected_owner_id",
        "relay_origin",
        "hpke_public",
        "challenge_sha256",
        "issued_ms",
        "expires_ms",
        "name",
        "handle",
        "definition_json",
    ] {
        let mut value = serde_json::to_value(&parsed).unwrap();
        let v = &mut value["offer"][field];
        *v = if v.is_number() {
            serde_json::json!(v.as_u64().unwrap() + 1)
        } else {
            serde_json::json!(format!("{}x", v.as_str().unwrap()))
        };
        let changed = format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap())
        );
        assert!(Code::parse(&changed, now()).is_err(), "{field}");
    }
    assert!(Code::parse(&format!("{PREFIX}{}", "A".repeat(CODE_LIMIT)), now()).is_err());
    assert!(Code::parse("zoen-link:1:ordinary-device-code", now()).is_err());
    assert!(owner
        .agent_enrollment_preview(
            &offer.code,
            &Signer::generate().unwrap().id(),
            &account.device_id
        )
        .is_err());
    let other = make_owner();
    let other_account = other.account().unwrap();
    assert!(other
        .agent_enrollment_preview(
            &offer.code,
            &other_account.identity_id,
            &other_account.device_id
        )
        .is_err());
}

#[test]
fn mailbox_envelope_requires_pinned_owner_full_signature_and_agent_context() {
    let owner = make_owner();
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    prepare(&owner, &host, &vault);
    let pending = Pending::load(&vault.vault()).unwrap().unwrap();
    let envelope = make_envelope(&owner, &pending);
    assert!(pending.open(&sealed(&pending, &envelope)).is_ok());
    let mut changed = envelope.clone();
    changed.offer_digest = "0".repeat(64);
    assert!(pending.open(&sealed(&pending, &changed)).is_err());
    let other = make_owner();
    let changed = make_envelope(&other, &pending);
    assert!(pending.open(&sealed(&pending, &changed)).is_err());
    let secret = canonical_secret(&pending.code.pairing_secret).unwrap();
    let normal_context = crate::link::seal(
        &hex::decode(&pending.code.offer.hpke_public).unwrap(),
        &secret,
        "identity",
        &serde_json::to_vec(&envelope).unwrap(),
    )
    .unwrap();
    assert!(pending.open(&normal_context).is_err());
    let ordinary_identity = serde_json::json!({"root_secret":"not-a-real-secret","device_cert":"synthetic","profile":{}});
    let bytes = crate::link::seal(
        &hex::decode(&pending.code.offer.hpke_public).unwrap(),
        &secret,
        &pending.code.box_context().unwrap(),
        &serde_json::to_vec(&ordinary_identity).unwrap(),
    )
    .unwrap();
    assert!(pending.open(&bytes).is_err());
    assert!(host.account().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn production_agent_wait_uses_own_pickup_auth_installs_verified_box_and_never_refetches_accepted(
) {
    use futures_util::{SinkExt, StreamExt};
    use protocol::{auth_message, ClientFrame, ServerFrame, PROTOCOL_VERSION};
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("http://{}", listener.local_addr().unwrap());
    let owner = Client::open(":memory:".into(), "en".into()).unwrap();
    owner
        .lock()
        .create_account("Synthetic owner", "fixture_owner", &relay)
        .unwrap();
    owner.lock().set_registered(true).unwrap(); // offline fixture, not relay proof
    let vault = DiskVault::new().unwrap();
    let host = vault.client();
    let mut setup = request(&owner);
    setup.relay_url = relay;
    let offer = host.prepare_agent_enrollment(setup, vault.vault()).unwrap();
    let account = owner.account().unwrap();
    assert!(owner
        .agent_enrollment_preview(&offer.code, &account.identity_id, &account.device_id)
        .is_ok());
    let pending = Pending::load(&vault.vault()).unwrap().unwrap();
    let response = sealed(&pending, &make_envelope(&owner, &pending));
    let expected_pickup = Signer::from_secret(&pending.pickup).id();
    let expected_box = crate::link::box_id(
        &canonical_secret(&pending.code.pairing_secret).unwrap(),
        &pending.code.box_context().unwrap(),
    );
    assert_ne!(expected_pickup, pending.code.offer.agent_id);
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let Message::Binary(frame) = socket.next().await.unwrap().unwrap() else {
            panic!("Hello");
        };
        let ClientFrame::Hello {
            identity,
            device,
            cert,
            ..
        } = ClientFrame::decode(&frame).unwrap()
        else {
            panic!("Hello");
        };
        assert_eq!(identity, expected_pickup);
        assert_eq!(device, expected_pickup);
        assert!(event_log::verify_sig(
            &device,
            &event_log::device_cert_message(&device),
            &cert
        ));
        let nonce = "synthetic-enrollment-test";
        let relay_name = "synthetic-enrollment-relay";
        socket
            .send(Message::Binary(
                ServerFrame::Challenge {
                    nonce: nonce.into(),
                    relay: relay_name.into(),
                    protocol: PROTOCOL_VERSION,
                    capabilities: Vec::new(),
                    server_time_ms: None,
                }
                .encode()
                .into(),
            ))
            .await
            .unwrap();
        let Message::Binary(frame) = socket.next().await.unwrap().unwrap() else {
            panic!("Auth");
        };
        let ClientFrame::Auth { sig } = ClientFrame::decode(&frame).unwrap() else {
            panic!("Auth");
        };
        assert!(event_log::verify_sig(
            &device,
            &auth_message(nonce, relay_name),
            &sig
        ));
        socket
            .send(Message::Binary(
                ServerFrame::Ready {
                    identity,
                    registered: false,
                }
                .encode()
                .into(),
            ))
            .await
            .unwrap();
        let Message::Binary(frame) = socket.next().await.unwrap().unwrap() else {
            panic!("FetchLink");
        };
        let ClientFrame::Req {
            id,
            op: Op::FetchLink { id: box_id },
        } = ClientFrame::decode(&frame).unwrap()
        else {
            panic!("only FetchLink allowed");
        };
        assert_eq!(box_id, expected_box);
        socket
            .send(Message::Binary(
                ServerFrame::Res {
                    id,
                    result: Ok(Reply::Link(Some(response))),
                }
                .encode()
                .into(),
            ))
            .await
            .unwrap();
        listener
    });
    let progress = tokio::time::timeout(
        Duration::from_secs(3),
        host.wait_agent_enrollment(vault.vault(), 2000),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(progress.agent_id, pending.code.offer.agent_id);
    assert!(!progress.registered);
    assert_eq!(
        host.account().unwrap().device_id,
        Signer::from_secret(&pending.device).id()
    );
    assert!(Pending::load(&vault.vault())
        .unwrap()
        .unwrap()
        .accepted
        .is_some());
    let listener = tokio::time::timeout(Duration::from_secs(3), peer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        host.wait_agent_enrollment(vault.vault(), 2000)
            .await
            .unwrap(),
        progress
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

mod durability_cases;

mod guard_cases;
