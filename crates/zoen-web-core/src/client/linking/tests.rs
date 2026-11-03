use super::*;
use crate::client::ServerFrame;
use event_log::{Author, Signer as Root};
use openmls_traits::{signatures::SignerError, types::SignatureScheme};
#[derive(Clone)]
struct HostSigner(Root);
impl Signer for HostSigner {
    fn sign(&self, value: &[u8]) -> Result<Vec<u8>, SignerError> {
        hex::decode(self.0.sign(value)).map_err(|_| SignerError::SigningError)
    }
    fn signature_scheme(&self) -> SignatureScheme {
        mls::SUITE.signature_algorithm()
    }
}
fn source() -> std::result::Result<(Primary, Client<HostSigner>), Box<dyn std::error::Error>> {
    let signer = HostSigner(Root::from_secret(&[101; 32]));
    let primary = Primary::new(&signer.0.id(), "http://localhost:8790", "Private owner")?;
    let mut client = Client::new(primary.account().clone(), signer, "")?;
    client.synced = true;
    Ok((primary, client))
}
#[test]
fn retained_request_allocator_is_exact_u32_and_refuses_exhaustion_without_mutation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_, mut client) = source()?;
    client.saved.linking.request_sequence = u64::from(u32::MAX) - LINK_REQUEST_BASE - 1;
    assert_eq!(client.next_retained_request_id()?, u64::from(u32::MAX));
    let before = client.saved.linking.request_sequence;
    assert!(client.next_retained_request_id().is_err());
    assert_eq!(client.saved.linking.request_sequence, before);
    client.saved.linking.request_sequence = u64::MAX;
    assert!(client.next_retained_request_id().is_err());
    assert_eq!(client.saved.linking.request_sequence, u64::MAX);

    Ok(())
}

fn prepare(
    primary: &Primary,
    client: &mut Client<HostSigner>,
    seed: u8,
) -> std::result::Result<(crate::PendingLink, crate::LinkedAccount, u64), Box<dyn std::error::Error>>
{
    let pending = crate::create_link(&Root::from_secret(&[seed; 32]).id(), &client.account.relay)?;
    let response = client.link_prepare(primary, &pending.code, 100)?;
    let update = response.update.ok_or("missing authenticated test update")?;
    let (id, sealed) = update
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .find_map(|frame| match frame {
            ClientFrame::Req {
                id,
                op: Op::DeliverLink { sealed, .. },
            } => Some((id, sealed)),
            _ => None,
        })
        .ok_or("missing authenticated test find_map value")?;
    let account = crate::complete_link(&pending, &sealed)?;
    Ok((pending, account, id))
}
fn ack(
    client: &mut Client<HostSigner>,
    id: u64,
    now: i64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    client.receive(
        &ServerFrame::Res {
            id,
            result: Ok(Reply::Done),
        }
        .encode(),
        now,
    )?;

    Ok(())
}
fn command(
    client: &mut Client<HostSigner>,
    request: &Value,
    now: i64,
) -> std::result::Result<DocumentResponse, Box<dyn std::error::Error>> {
    Ok(client.link_request(&request.to_string(), now)?)
}

#[test]
fn handover_cold_retry_reuses_ciphertext_without_retaining_root_and_ignores_old_ack(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (primary, mut client) = source()?;
    let (pending, linked, id) = prepare(&primary, &mut client, 102)?;
    assert_eq!(linked.identity, client.account.identity);
    assert_eq!(linked.primary, client.account.device);
    let first = client
        .saved
        .linking
        .handover
        .as_ref()
        .ok_or("missing authenticated test as_ref value")?
        .sealed
        .clone();
    let snapshot = client.update(Vec::new())?.snapshot;
    assert!(!snapshot.contains(&hex::encode(primary.authority_bytes())));
    assert!(!snapshot.contains("identity_secret"));
    let mut resumed = Client::new(client.account.clone(), client.signer.clone(), &snapshot)?;
    resumed.synced = true;
    let retry = resumed.link_prepare(&primary, &pending.code, 200)?;
    assert_eq!(
        resumed
            .saved
            .linking
            .handover
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .sealed,
        first
    );
    assert!(retry.update.ok_or("missing authenticated test update")?.frames.iter().any(|frame|matches!(ClientFrame::decode(frame).map_err(|error| error.to_string()),Ok(ClientFrame::Req{id:retry_id,..})if retry_id==id)));
    assert!(!resumed.saved.linking.certified.contains(&linked.device));
    ack(&mut resumed, id, 201)?;
    assert!(resumed.saved.linking.certified.contains(&linked.device));
    assert_eq!(
        resumed
            .saved
            .linking
            .handover
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .sealed,
        Vec::<u8>::new()
    );
    let (_, other, new_id) = prepare(&primary, &mut resumed, 103)?;
    assert_ne!(id, new_id);
    ack(&mut resumed, id, 202)?;
    assert!(!resumed.saved.linking.certified.contains(&other.device));
    assert!(
        !resumed
            .saved
            .linking
            .handover
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .delivered
    );
    let wrong = Primary::new(
        &Root::from_secret(&[104; 32]).id(),
        "http://localhost:8790",
        "Other",
    )?;
    assert!(wrong
        .seal_link(
            &device_link::LinkCode::parse(&pending.code)
                .ok_or("authenticated fixture link code invalid")?,
            &client.account
        )
        .is_err());

    Ok(())
}

#[test]
fn outgoing_history_retains_exact_ciphertext_until_ack_and_native_wire_opens_it(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = OutgoingFixture::new()?;
    fixture.retry_survives_failure()?;
    let request = fixture.verify_uploaded_manifest()?;
    fixture.finish(request)?;
    Ok(())
}

struct OutgoingFixture {
    resumed: Client<HostSigner>,
    pending: crate::PendingLink,
    linked: crate::LinkedAccount,
    work: Value,
    sealed: Vec<u8>,
}

impl OutgoingFixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (primary, mut client) = source()?;
        let (pending, linked, id) = prepare(&primary, &mut client, 105)?;
        assert!(client
            .link_request(
                &json!({"op":"history_send","device_id":linked.device}).to_string(),
                101
            )
            .is_err());
        ack(&mut client, id, 102)?;
        let first = command(
            &mut client,
            &json!({"op":"history_send","device_id":linked.device}),
            103,
        )?;
        let work = first.result;
        let sealed: Vec<u8> = serde_json::from_value(work["bytes"].clone())?;
        assert_eq!(work["kind"], "upload");
        let payload = protocol::transfer_message(
            "put",
            work["transfer"]
                .as_str()
                .ok_or("missing authenticated test as_str value")?,
            "0",
            work["sha256"]
                .as_str()
                .ok_or("missing authenticated test as_str value")?,
            103,
        );
        assert!(event_log::verify_sig(
            &client.account.device,
            &payload,
            work["signature"]
                .as_str()
                .ok_or("missing authenticated test as_str value")?
        ));
        let mut resumed = Client::new(
            client.account.clone(),
            client.signer.clone(),
            &first
                .update
                .ok_or("missing authenticated test update")?
                .snapshot,
        )?;
        resumed.synced = true;
        Ok(Self {
            resumed,
            pending,
            linked,
            work,
            sealed,
        })
    }
    fn retry_survives_failure(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let again = command(&mut self.resumed, &json!({"op":"history_send_resume"}), 104)?.result;
        assert_eq!(again["bytes"], self.work["bytes"]);
        assert_eq!(again["transfer"], self.work["transfer"]);
        let transfer = self.work["transfer"]
            .as_str()
            .ok_or("missing authenticated test as_str value")?;
        let refused = command(
            &mut self.resumed,
            &json!({"op":"history_failed","transfer":transfer,"message":"Server refused"}),
            104,
        )?;
        assert_eq!(refused.result["kind"], "failed");
        let snapshot = refused
            .update
            .ok_or("missing authenticated test update")?
            .snapshot;
        self.resumed = Client::new(
            self.resumed.account.clone(),
            self.resumed.signer.clone(),
            &snapshot,
        )?;
        self.resumed.synced = true;
        assert!(self
            .resumed
            .link_request(&json!({"op":"history_send_resume"}).to_string(), 104)
            .is_err());
        let retried = command(
            &mut self.resumed,
            &json!({"op":"history_send","device_id":self.linked.device}),
            104,
        )?
        .result;
        assert_eq!(retried["bytes"], self.work["bytes"]);
        assert_eq!(retried["transfer"], self.work["transfer"]);

        Ok(())
    }
    fn verify_uploaded_manifest(&mut self) -> Result<u64, Box<dyn std::error::Error>> {
        let transfer = self.work["transfer"]
            .as_str()
            .ok_or("authenticated transfer id missing")?;
        assert!(self.resumed.link_request(&json!({"op":"history_uploaded","transfer":transfer,"index":0,"sha256":"00".repeat(32)}).to_string(),105).is_err());
        let uploaded = command(
            &mut self.resumed,
            &json!({"op":"history_uploaded","transfer":transfer,"index":0,"sha256":self.work["sha256"]}),
            106,
        )?;
        let (request, manifest) = uploaded
            .update
            .ok_or("missing authenticated test update")?
            .frames
            .iter()
            .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .find_map(|frame| match frame {
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
            .ok_or("missing authenticated test find_map value")?;
        let plain = device_link::open(
            &hex::decode(&self.pending.hpke_secret)?,
            &hex::decode(&self.pending.secret)?
                .try_into()
                .map_err(|_| "authenticated fixture key length invalid")?,
            "history",
            &manifest,
        )
        .ok_or("authenticated test cryptographic evidence missing")?;
        let manifest: device_link::HistoryBox = serde_json::from_slice(&plain)?;
        assert_eq!(manifest.transfer, transfer);
        assert_eq!(manifest.chunks[0], event_log::sha256_hex(&self.sealed));
        let key = hex::decode(manifest.key)?
            .try_into()
            .map_err(|_| "authenticated fixture key length invalid")?;
        let bundle: device_link::Bundle = serde_json::from_slice(
            &device_link::open_chunk(&key, 0, &self.sealed)
                .ok_or("authenticated history chunk failed to open")?,
        )?;
        assert_eq!(bundle.events, []);
        assert_eq!(
            command(&mut self.resumed, &json!({"op":"history_send_resume"}), 107)?.result["kind"],
            "waiting"
        );
        Ok(request)
    }
    fn finish(mut self, request: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.resumed
            .saved
            .membership
            .revoked
            .insert(self.linked.device.clone());
        self.resumed.linking_net.history_due = 0;
        let after_revoke = self.resumed.maintenance(108)?;
        let after_revoke_frames = after_revoke
            .frames
            .iter()
            .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        assert!(!after_revoke_frames.iter().any(|frame| matches!(
            frame,
            ClientFrame::Req {
                op: Op::DeliverLink { device: None, .. },
                ..
            }
        )));
        assert!(self
            .resumed
            .link_request(&json!({"op":"history_send_resume"}).to_string(), 108)
            .is_err());
        self.resumed.saved.membership.revoked.clear();
        let delivered = self.resumed.receive(
            &ServerFrame::Res {
                id: request,
                result: Ok(Reply::Done),
            }
            .encode(),
            108,
        )?;
        let hint = delivered
            .frames
            .iter()
            .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .find_map(|frame| match frame {
                ClientFrame::Req {
                    op: Op::SendDevice { to, sealed },
                    ..
                } if to == self.linked.device => Some(sealed),
                _ => None,
            })
            .ok_or("missing authenticated test find_map value")?;
        let receiver = Peer {
            device: self.resumed.account.device.clone(),
            their_hpke: String::new(),
            my_hpke_secret: self.pending.hpke_secret.clone(),
            secret: self.pending.secret,
        };
        assert!(matches!(
            receiver.open(&hint),
            Some(DeviceMsg::HistoryReady)
        ));
        assert_eq!(
            command(&mut self.resumed, &json!({"op":"history_send_resume"}), 109)?.result["kind"],
            "complete"
        );
        assert_eq!(
            self.resumed
                .saved
                .linking
                .sending
                .as_ref()
                .ok_or("missing authenticated test as_ref value")?
                .chunks,
            Vec::<Vec<u8>>::new()
        );

        Ok(())
    }
}

mod page_history;
