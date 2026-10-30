use super::*;
use event_log::{Author, Signer as NativeSigner};
use openmls_traits::{signatures::SignerError, types::SignatureScheme};

#[derive(Clone)]
struct HostSigner(NativeSigner);
impl Signer for HostSigner {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        Ok(hex::decode(self.0.sign(payload)).unwrap())
    }
    fn signature_scheme(&self) -> SignatureScheme {
        mls::SUITE.signature_algorithm()
    }
}

struct Fixture {
    account: LinkedAccount,
    signer: HostSigner,
    public: Vec<u8>,
    author: Author,
    log: SpaceLog,
    opened: Vec<Event>,
}
fn fixture() -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    let identity = NativeSigner::from_secret(&[41; 32]);
    let device = NativeSigner::from_secret(&[42; 32]);
    let author = Author::device(&identity, NativeSigner::from_secret(&[43; 32]));
    let (private, public) = device_link::key_pair()?;
    let account = LinkedAccount {
        identity: identity.id(),
        device: device.id(),
        cert: identity.sign(&event_log::device_cert_message(&device.id())),
        name: "Owner".into(),
        handle: "owner".into(),
        relay: "http://localhost:8080".into(),
        primary: author.device.clone().unwrap(),
        primary_hpke: String::new(),
        hpke_secret: hex::encode(private),
        link_secret: hex::encode([44; 32]),
        agreement_secret: String::new(),
        profile_key: String::new(),
        profile_key_version: 0,
        profile_seen_version: 0,
        profile_fields: String::new(),
    };
    let mut log = SpaceLog::new("sp_history");
    log.append(
        &identity,
        1,
        EventBody::SpaceCreated {
            title: "Private".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?;
    let mut opened = Vec::new();
    for now in 2..4 {
        let id = format!("msg_{now}");
        let mut inner = author.sign_event(
            log.space(),
            &id,
            now,
            log.head(),
            EventBody::MessagePosted {
                message: id.clone(),
                text: format!("History {now}"),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
        )?;
        let (content, sig) = author.sign_sealed(
            log.space(),
            &id,
            now,
            log.head().as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, vec![now as u8]),
        );
        let event = event_log::event_from_content(
            content,
            sig,
            author.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .unwrap();
        let event = log.sequence(event)?;
        inner.seq = event.seq;
        inner.sealed_wire = Some(event_log::content_hash_of(event));
        opened.push(inner);
    }
    Ok(Fixture {
        account,
        signer: HostSigner(device),
        public,
        author,
        log,
        opened,
    })
}

#[test]
fn history_ready_only_wakes_from_the_pinned_primary_authenticated_device_channel(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture()?;
    let peer = device_link::Peer {
        device: f.account.device.clone(),
        their_hpke: hex::encode(&f.public),
        my_hpke_secret: String::new(),
        secret: f.account.link_secret.clone(),
    };
    let sealed = peer.seal(&device_link::DeviceMsg::HistoryReady).unwrap();
    let mut client = Client::new(f.account.clone(), f.signer, "").unwrap();
    for (from, to) in [
        ("wrong", f.account.device.as_str()),
        (f.account.primary.as_str(), "wrong"),
    ] {
        let update = client
            .receive(
                &ServerFrame::DeviceMessage {
                    from: from.into(),
                    to: to.into(),
                    sealed: sealed.clone(),
                }
                .encode(),
                1,
            )
            .unwrap();
        assert!(!update.history_ready);
    }
    let mut forged = sealed.clone();
    *forged.last_mut().unwrap() ^= 1;
    assert!(!client.history_hint(&f.account.primary, &f.account.device, &forged));
    let update = client
        .receive(
            &ServerFrame::DeviceMessage {
                from: f.account.primary,
                to: f.account.device,
                sealed,
            }
            .encode(),
            2,
        )
        .unwrap();
    assert!(update.history_ready);
    assert!(
        !client.update(Vec::new()).unwrap().history_ready,
        "wake hints are transient, not durable restore state"
    );
    assert!(client.saved.history.transfer.is_none());
    Ok(())
}

#[test]
fn native_history_resumes_verifies_before_import_and_persists_before_cleanup(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture()?;
    let mut client = Client::new(f.account.clone(), f.signer.clone(), "").unwrap();
    client.logs.insert(f.log.space().into(), f.log.clone());
    let status: serde_json::Value = serde_json::from_str(&client.history_status()).unwrap();
    assert_eq!(status["initial_pending"], true);
    assert_eq!(status["manifest_received"], false);
    assert!(matches!(
        client.history_start(5).unwrap().work,
        HistoryWork::FetchManifest { .. }
    ));
    let plain = serde_json::to_vec(&Bundle {
        events: f.opened.clone(),
    })
    .unwrap();
    let key = [45; 32];
    let chunks = plain
        .chunks(plain.len() / 2 + 1)
        .enumerate()
        .map(|(n, chunk)| device_link::seal_chunk(&key, n as u32, chunk))
        .collect::<Result<Vec<_>, _>>()?;
    let manifest = HistoryBox {
        transfer: "ab".repeat(32),
        key: hex::encode(key),
        chunks: chunks
            .iter()
            .map(|chunk| event_log::sha256_hex(chunk))
            .collect(),
    };
    let sealed = device_link::seal(
        &f.public,
        &[44; 32],
        "history",
        &serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        client.history_manifest(&sealed, 6).unwrap().work,
        HistoryWork::Download { index: 0, .. }
    ));
    let mut corrupt = chunks[0].clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(client.history_chunk(0, &corrupt, 7).is_err());
    assert!(client
        .saved
        .history
        .transfer
        .as_ref()
        .unwrap()
        .chunks
        .is_empty());
    let partial = client.history_chunk(0, &chunks[0], 8).unwrap();
    let mut resumed = Client::new(
        f.account.clone(),
        f.signer.clone(),
        &partial.update.snapshot,
    )
    .unwrap();
    let status: serde_json::Value = serde_json::from_str(&resumed.history_status()).unwrap();
    assert_eq!(status["initial_pending"], true);
    assert_eq!(status["manifest_received"], true);
    assert_eq!(status["chunks_received"], 1);
    assert_eq!(status["chunks_total"], 2);
    resumed.history_start(9).unwrap();
    let waiting = resumed.history_chunk(1, &chunks[1], 10).unwrap();
    assert!(matches!(waiting.work, HistoryWork::WaitingSync));
    assert!(matches!(
        resumed.logs[f.log.space()].events()[1].body,
        EventBody::Sealed { .. }
    ));
    resumed.synced = true;
    let restored = resumed.history_finish(11).unwrap();
    let HistoryWork::Complete {
        messages: 2,
        chunks: 2,
        resumed: 1,
        cleanup: Some(cleanup),
    } = restored.work
    else {
        panic!("actual verified import expected")
    };
    assert!(event_log::verify_sig(
        &f.account.device,
        &protocol::transfer_message("delete", &manifest.transfer, "", "", 11),
        &cleanup.signature
    ));
    assert!(resumed.saved.history.transfer.is_none());
    resumed.logs[f.log.space()].verify().unwrap();
    assert!(
        matches!(&resumed.logs[f.log.space()].events()[1].body,EventBody::MessagePosted {text,..} if text == "History 2")
    );
    let mut reloaded = Client::new(f.account, f.signer, &restored.update.snapshot).unwrap();
    let status: serde_json::Value = serde_json::from_str(&reloaded.history_status()).unwrap();
    assert_eq!(status["initial_pending"], false);
    assert_eq!(status["chunks_received"], 2);
    assert!(matches!(
        reloaded.history_finish(12).unwrap().work,
        HistoryWork::Complete {
            messages: 2,
            cleanup: Some(_),
            ..
        }
    ));
    assert!(matches!(
        reloaded.history_cleaned(13).unwrap().work,
        HistoryWork::Complete {
            messages: 2,
            cleanup: None,
            ..
        }
    ));
    Ok(())
}

#[test]
fn one_mismatched_signed_inner_cannot_partially_replace_the_relay_log(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture()?;
    let mut client = Client::new(f.account, f.signer, "").unwrap();
    client.logs.insert(f.log.space().into(), f.log.clone());
    let original = f.log.events().to_vec();
    let event = &f.opened[1];
    let mut forged = f.author.sign_event(
        &event.space,
        &event.client_id,
        999,
        event.seen.clone(),
        event.body.clone(),
    )?;
    forged.seq = event.seq;
    forged.sealed_wire = event.sealed_wire.clone();
    assert!(client
        .import_history_events(vec![f.opened[0].clone(), forged])
        .is_err());
    assert_eq!(client.logs[f.log.space()].events(), original);
    assert_eq!(client.import_history_events(f.opened.clone()).unwrap(), 2);
    assert_eq!(client.import_history_events(f.opened).unwrap(), 0);
    Ok(())
}
