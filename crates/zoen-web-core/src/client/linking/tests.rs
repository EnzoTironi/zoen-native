use super::*;
use event_log::{Author, Signer as Root};
use openmls_traits::{signatures::SignerError, types::SignatureScheme};
#[derive(Clone)]
struct HostSigner(Root);
impl Signer for HostSigner {
    fn sign(&self, value: &[u8]) -> Result<Vec<u8>, SignerError> {
        Ok(hex::decode(self.0.sign(value)).unwrap())
    }
    fn signature_scheme(&self) -> SignatureScheme {
        mls::SUITE.signature_algorithm()
    }
}
fn source() -> (Primary, Client<HostSigner>) {
    let signer = HostSigner(Root::from_secret(&[101; 32]));
    let primary = Primary::new(&signer.0.id(), "http://localhost:8790", "Private owner").unwrap();
    let mut client = Client::new(primary.account().clone(), signer, "").unwrap();
    client.synced = true;
    (primary, client)
}
#[test]
fn retained_request_allocator_is_exact_u32_and_refuses_exhaustion_without_mutation() {
    let (_, mut client) = source();
    client.saved.linking.request_sequence = u64::from(u32::MAX) - LINK_REQUEST_BASE - 1;
    assert_eq!(
        client.next_retained_request_id().unwrap(),
        u64::from(u32::MAX)
    );
    let before = client.saved.linking.request_sequence;
    assert!(client.next_retained_request_id().is_err());
    assert_eq!(client.saved.linking.request_sequence, before);
    client.saved.linking.request_sequence = u64::MAX;
    assert!(client.next_retained_request_id().is_err());
    assert_eq!(client.saved.linking.request_sequence, u64::MAX);
}

fn prepare(
    primary: &Primary,
    client: &mut Client<HostSigner>,
    seed: u8,
) -> (crate::PendingLink, crate::LinkedAccount, u64) {
    let pending =
        crate::create_link(&Root::from_secret(&[seed; 32]).id(), &client.account.relay).unwrap();
    let response = client.link_prepare(primary, &pending.code, 100).unwrap();
    let update = response.update.unwrap();
    let (id, sealed) = update
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                id,
                op: Op::DeliverLink { sealed, .. },
            } => Some((id, sealed)),
            _ => None,
        })
        .unwrap();
    let account = crate::complete_link(&pending, &sealed).unwrap();
    (pending, account, id)
}
fn ack(client: &mut Client<HostSigner>, id: u64, now: i64) {
    client
        .receive(
            &ServerFrame::Res {
                id,
                result: Ok(Reply::Done),
            }
            .encode(),
            now,
        )
        .unwrap();
}
fn command(client: &mut Client<HostSigner>, request: Value, now: i64) -> DocumentResponse {
    client.link_request(&request.to_string(), now).unwrap()
}

#[test]
fn handover_cold_retry_reuses_ciphertext_without_retaining_root_and_ignores_old_ack() {
    let (primary, mut client) = source();
    let (pending, linked, id) = prepare(&primary, &mut client, 102);
    assert_eq!(linked.identity, client.account.identity);
    assert_eq!(linked.primary, client.account.device);
    let first = client
        .saved
        .linking
        .handover
        .as_ref()
        .unwrap()
        .sealed
        .clone();
    let snapshot = client.update(Vec::new()).unwrap().snapshot;
    assert!(!snapshot.contains(&hex::encode(primary.authority_bytes())));
    assert!(!snapshot.contains("identity_secret"));
    let mut resumed =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    resumed.synced = true;
    let retry = resumed.link_prepare(&primary, &pending.code, 200).unwrap();
    assert_eq!(
        resumed.saved.linking.handover.as_ref().unwrap().sealed,
        first
    );
    assert!(retry.update.unwrap().frames.iter().any(|frame|matches!(ClientFrame::decode(frame),Ok(ClientFrame::Req{id:retry_id,..})if retry_id==id)));
    assert!(!resumed.saved.linking.certified.contains(&linked.device));
    ack(&mut resumed, id, 201);
    assert!(resumed.saved.linking.certified.contains(&linked.device));
    assert!(resumed
        .saved
        .linking
        .handover
        .as_ref()
        .unwrap()
        .sealed
        .is_empty());
    let (_, other, new_id) = prepare(&primary, &mut resumed, 103);
    assert_ne!(id, new_id);
    ack(&mut resumed, id, 202);
    assert!(!resumed.saved.linking.certified.contains(&other.device));
    assert!(!resumed.saved.linking.handover.as_ref().unwrap().delivered);
    let wrong = Primary::new(
        &Root::from_secret(&[104; 32]).id(),
        "http://localhost:8790",
        "Other",
    )
    .unwrap();
    assert!(wrong
        .seal_link(
            &device_link::LinkCode::parse(&pending.code).unwrap(),
            &client.account
        )
        .is_err());
}

#[test]
fn outgoing_history_retains_exact_ciphertext_until_ack_and_native_wire_opens_it() {
    let (primary, mut client) = source();
    let (pending, linked, id) = prepare(&primary, &mut client, 105);
    assert!(client
        .link_request(
            &json!({"op":"history_send","device_id":linked.device}).to_string(),
            101
        )
        .is_err());
    ack(&mut client, id, 102);
    let first = command(
        &mut client,
        json!({"op":"history_send","device_id":linked.device}),
        103,
    );
    let work = first.result;
    let sealed: Vec<u8> = serde_json::from_value(work["bytes"].clone()).unwrap();
    assert_eq!(work["kind"], "upload");
    let payload = protocol::transfer_message(
        "put",
        work["transfer"].as_str().unwrap(),
        "0",
        work["sha256"].as_str().unwrap(),
        103,
    );
    assert!(event_log::verify_sig(
        &client.account.device,
        &payload,
        work["signature"].as_str().unwrap()
    ));
    let mut resumed = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &first.update.unwrap().snapshot,
    )
    .unwrap();
    resumed.synced = true;
    let again = command(&mut resumed, json!({"op":"history_send_resume"}), 104).result;
    assert_eq!(again["bytes"], work["bytes"]);
    assert_eq!(again["transfer"], work["transfer"]);
    let transfer = work["transfer"].as_str().unwrap();
    let refused = command(
        &mut resumed,
        json!({"op":"history_failed","transfer":transfer,"message":"Server refused"}),
        104,
    );
    assert_eq!(refused.result["kind"], "failed");
    let snapshot = refused.update.unwrap().snapshot;
    resumed = Client::new(resumed.account.clone(), resumed.signer.clone(), &snapshot).unwrap();
    resumed.synced = true;
    assert!(resumed
        .link_request(&json!({"op":"history_send_resume"}).to_string(), 104)
        .is_err());
    let retried = command(
        &mut resumed,
        json!({"op":"history_send","device_id":linked.device}),
        104,
    )
    .result;
    assert_eq!(retried["bytes"], work["bytes"]);
    assert_eq!(retried["transfer"], work["transfer"]);

    assert!(resumed.link_request(&json!({"op":"history_uploaded","transfer":transfer,"index":0,"sha256":"00".repeat(32)}).to_string(),105).is_err());
    let uploaded = command(
        &mut resumed,
        json!({"op":"history_uploaded","transfer":transfer,"index":0,"sha256":work["sha256"]}),
        106,
    );
    let (request, manifest) = uploaded
        .update
        .unwrap()
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                id,
                op:
                    Op::DeliverLink {
                        sealed,
                        device: None,
                        ..
                    },
            } => Some((id, sealed)),
            _ => None,
        })
        .unwrap();
    let plain = device_link::open(
        &hex::decode(&pending.hpke_secret).unwrap(),
        &hex::decode(&pending.secret).unwrap().try_into().unwrap(),
        "history",
        &manifest,
    )
    .unwrap();
    let manifest: device_link::HistoryBox = serde_json::from_slice(&plain).unwrap();
    assert_eq!(manifest.transfer, transfer);
    assert_eq!(manifest.chunks[0], event_log::sha256_hex(&sealed));
    let key = hex::decode(manifest.key).unwrap().try_into().unwrap();
    let bundle: device_link::Bundle =
        serde_json::from_slice(&device_link::open_chunk(&key, 0, &sealed).unwrap()).unwrap();
    assert!(bundle.events.is_empty());
    assert_eq!(
        command(&mut resumed, json!({"op":"history_send_resume"}), 107).result["kind"],
        "waiting"
    );
    resumed
        .saved
        .membership
        .revoked
        .insert(linked.device.clone());
    resumed.linking_net.history_due = 0;
    let after_revoke = resumed.maintenance(108).unwrap();
    assert!(!after_revoke.frames.iter().any(|frame| matches!(
        ClientFrame::decode(frame),
        Ok(ClientFrame::Req {
            op: Op::DeliverLink { device: None, .. },
            ..
        })
    )));
    assert!(resumed
        .link_request(&json!({"op":"history_send_resume"}).to_string(), 108)
        .is_err());
    resumed.saved.membership.revoked.clear();
    let delivered = resumed
        .receive(
            &ServerFrame::Res {
                id: request,
                result: Ok(Reply::Done),
            }
            .encode(),
            108,
        )
        .unwrap();
    let hint = delivered
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                op: Op::SendDevice { to, sealed },
                ..
            } if to == linked.device => Some(sealed),
            _ => None,
        })
        .unwrap();
    let receiver = Peer {
        device: resumed.account.device.clone(),
        their_hpke: String::new(),
        my_hpke_secret: pending.hpke_secret.clone(),
        secret: pending.secret.clone(),
    };
    assert!(matches!(
        receiver.open(&hint),
        Some(DeviceMsg::HistoryReady)
    ));
    assert_eq!(
        command(&mut resumed, json!({"op":"history_send_resume"}), 109).result["kind"],
        "complete"
    );
    assert!(resumed
        .saved
        .linking
        .sending
        .as_ref()
        .unwrap()
        .chunks
        .is_empty());
}

#[test]
fn older_page_requires_matching_peer_request_space_and_signed_relay_record(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (primary, mut source) = source();
    let (_, linked, id) = prepare(&primary, &mut source, 106);
    ack(&mut source, id, 101);
    let identity = Root::from_secret(&primary.authority_bytes().try_into().unwrap());
    let author = Author::device(&identity, source.signer.0.clone());
    let space = "sp_older";
    let mut log = SpaceLog::new(space);
    log.append(
        &identity,
        1,
        EventBody::SpaceCreated {
            title: "Private".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?;
    let mut opened = author.sign_event(
        space,
        "msg_old",
        2,
        log.head(),
        EventBody::MessagePosted {
            message: "msg_old".into(),
            text: "Actual old message".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
    )?;
    let (content, sig) = author.sign_sealed(
        space,
        "msg_old",
        2,
        log.head().as_ref(),
        Sealed::new(SealedKind::Application, SUITE_ID, vec![1]),
    );
    let outer = event_log::event_from_content(
        content,
        sig,
        author.cert.clone(),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    let outer = log.sequence(outer)?.clone();
    opened.seq = outer.seq;
    opened.sealed_wire = Some(event_log::content_hash_of(&outer));
    let opened = device_link::opened_history_event(&outer, opened).unwrap();
    let mut source_events = log.events().to_vec();
    source_events[1] = opened.clone();
    source.logs.insert(
        space.into(),
        SpaceLog::from_events(space, source_events).unwrap(),
    );
    let mut destination = Client::new(
        linked.clone(),
        HostSigner(Root::from_secret(&[106; 32])),
        "",
    )
    .unwrap();
    destination.synced = true;
    destination.logs.insert(space.into(), log);
    let request = command(
        &mut destination,
        json!({"op":"load_older","space_id":space}),
        200,
    );
    let req = request.result["request_id"].as_str().unwrap();
    let sealed = request
        .update
        .unwrap()
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                op: Op::SendDevice { sealed, .. },
                ..
            } => Some(sealed),
            _ => None,
        })
        .unwrap();
    let answer = source
        .receive(
            &ServerFrame::DeviceMessage {
                from: linked.device.clone(),
                to: source.account.device.clone(),
                sealed,
            }
            .encode(),
            201,
        )
        .unwrap();
    let sealed = answer
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                op: Op::SendDevice { sealed, .. },
                ..
            } => Some(sealed),
            _ => None,
        })
        .unwrap();
    // A valid HPKE sender still cannot substitute a different Space or request.
    let peer = &source.saved.linking.peers[&linked.device];
    for (request, space) in [("other", space), (req, "sp_other")] {
        let sealed = peer
            .seal(&DeviceMsg::Page {
                req: request.into(),
                space: space.into(),
                events: vec![opened.clone()],
                more: false,
            })
            .unwrap();
        destination
            .receive(
                &ServerFrame::DeviceMessage {
                    from: source.account.device.clone(),
                    to: linked.device.clone(),
                    sealed,
                }
                .encode(),
                202,
            )
            .unwrap();
        assert!(destination.saved.linking.pages["sp_older"].answer.is_none());
    }
    destination
        .receive(
            &ServerFrame::DeviceMessage {
                from: source.account.device.clone(),
                to: linked.device.clone(),
                sealed,
            }
            .encode(),
            203,
        )
        .unwrap();
    assert_eq!(
        destination.saved.linking.pages[space].answer,
        Some((1, false))
    );
    assert!(destination.saved.linking.complete.contains(space));
    let snapshot = destination.update(Vec::new()).unwrap().snapshot;
    let resumed = Client::new(
        linked.clone(),
        HostSigner(Root::from_secret(&[106; 32])),
        &snapshot,
    )
    .unwrap();
    assert!(
        matches!(&resumed.logs[space].events()[1].body,EventBody::MessagePosted{text,..}if text=="Actual old message")
    );
    assert!(resumed.saved.linking.complete.contains(space));

    // JSON escaping may make one legitimate opened record larger than the
    // authenticated device-channel budget. Report it instead of retrying the
    // same empty boundary forever or claiming the history is complete.
    let mut large = author.sign_event(
        space,
        "msg_old",
        2,
        outer.seen.clone(),
        EventBody::MessagePosted {
            message: "msg_old".into(),
            text: "\n".repeat(600_000),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
    )?;
    large.seq = outer.seq;
    large.sealed_wire = Some(event_log::content_hash_of(&outer));
    let large = device_link::opened_history_event(&outer, large).unwrap();
    let create = source.logs[space].events()[0].clone();
    source.logs.insert(
        space.into(),
        SpaceLog::from_events(space, vec![create.clone(), large]).unwrap(),
    );
    let mut destination = Client::new(
        linked.clone(),
        HostSigner(Root::from_secret(&[106; 32])),
        "",
    )
    .unwrap();
    destination.synced = true;
    destination.logs.insert(
        space.into(),
        SpaceLog::from_events(space, vec![create, outer]).unwrap(),
    );
    let request = command(
        &mut destination,
        json!({"op":"load_older","space_id":space}),
        400,
    );
    let req = request.result["request_id"].as_str().unwrap();
    let sealed = request
        .update
        .unwrap()
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                op: Op::SendDevice { sealed, .. },
                ..
            } => Some(sealed),
            _ => None,
        })
        .unwrap();
    let response = source
        .receive(
            &ServerFrame::DeviceMessage {
                from: linked.device.clone(),
                to: source.account.device.clone(),
                sealed,
            }
            .encode(),
            401,
        )
        .unwrap();
    let sealed = response
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                op: Op::SendDevice { sealed, .. },
                ..
            } => Some(sealed),
            _ => None,
        })
        .unwrap();
    let update = destination
        .receive(
            &ServerFrame::DeviceMessage {
                from: source.account.device.clone(),
                to: linked.device,
                sealed,
            }
            .encode(),
            402,
        )
        .unwrap();
    assert!(!destination.saved.linking.complete.contains(space));
    let status = command(
        &mut destination,
        json!({"op":"older_status","space_id":space,"request_id":req}),
        403,
    )
    .result;
    assert_eq!(status["pending"], false);
    assert!(status["failure"]
        .as_str()
        .unwrap()
        .contains("grande demais"));
    assert!(update.maintenance_ms.is_none_or(|deadline| deadline > 0));
    assert!(!destination
        .maintenance(10_000)
        .unwrap()
        .frames
        .iter()
        .any(|frame| matches!(
            ClientFrame::decode(frame),
            Ok(ClientFrame::Req {
                op: Op::SendDevice { .. },
                ..
            })
        )));
    Ok(())
}
