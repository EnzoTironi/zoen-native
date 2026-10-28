use super::*;
use crate::client::{Sealed, SealedKind, ServerFrame, SpaceKind, SpaceLog, SUITE_ID};

#[test]
fn older_page_requires_matching_peer_request_space_and_signed_relay_record(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = OlderFixture::new()?;
    fixture.assert_bound_authenticated_page()?;
    fixture.assert_oversize_refuses_completion_and_retry_loop()?;
    Ok(())
}

struct OlderFixture {
    source: Client<HostSigner>,
    destination: Client<HostSigner>,
    linked: crate::LinkedAccount,
    author: Author,
    outer: crate::client::Event,
    opened: crate::client::Event,
}

impl OlderFixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (primary, mut source) = source()?;
        let (_, linked, id) = prepare(&primary, &mut source, 106)?;
        ack(&mut source, id, 101)?;
        let identity = Root::from_secret(
            &primary
                .authority_bytes()
                .try_into()
                .map_err(|_| "authenticated fixture key length invalid")?,
        );
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
        )?;
        let outer = log.sequence(outer)?.clone();
        opened.seq = outer.seq;
        opened.sealed_wire = Some(event_log::content_hash_of(&outer));
        let opened = device_link::opened_history_event(&outer, opened)
            .ok_or("authenticated opened history event mismatch")?;
        let mut source_events = log.events().to_vec();
        source_events[1] = opened.clone();
        source
            .logs
            .insert(space.into(), SpaceLog::from_events(space, source_events)?);
        let mut destination = Client::new(
            linked.clone(),
            HostSigner(Root::from_secret(&[106; 32])),
            "",
        )?;
        destination.synced = true;
        destination.logs.insert(space.into(), log);
        Ok(Self {
            source,
            destination,
            linked,
            author,
            outer,
            opened,
        })
    }
    fn request_page(&mut self, now: i64) -> Result<(String, Vec<u8>), Box<dyn std::error::Error>> {
        let space = "sp_older";
        let request = command(
            &mut self.destination,
            &json!({"op":"load_older","space_id":space}),
            now,
        )?;
        let req = request.result["request_id"]
            .as_str()
            .ok_or("missing authenticated test as_str value")?
            .to_owned();
        let sealed =
            device_ciphertext(&request.update.ok_or("missing authenticated test update")?)?;
        let answer = self.source.receive(
            &ServerFrame::DeviceMessage {
                from: self.linked.device.clone(),
                to: self.source.account.device.clone(),
                sealed,
            }
            .encode(),
            now + 1,
        )?;
        let sealed = device_ciphertext(&answer)?;
        Ok((req, sealed))
    }
    fn assert_bound_authenticated_page(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let space = "sp_older";
        let (req, sealed) = self.request_page(200)?;
        // A valid HPKE sender still cannot substitute a different Space or request.
        let peer = &self.source.saved.linking.peers[&self.linked.device];
        for (request, space) in [("other", space), (req.as_str(), "sp_other")] {
            let sealed = peer
                .seal(&DeviceMsg::Page {
                    req: request.into(),
                    space: space.into(),
                    events: vec![self.opened.clone()],
                    more: false,
                })
                .ok_or("authenticated history page sealing failed")?;
            self.destination.receive(
                &ServerFrame::DeviceMessage {
                    from: self.source.account.device.clone(),
                    to: self.linked.device.clone(),
                    sealed,
                }
                .encode(),
                202,
            )?;
            assert!(self.destination.saved.linking.pages["sp_older"]
                .answer
                .is_none());
        }
        self.destination.receive(
            &ServerFrame::DeviceMessage {
                from: self.source.account.device.clone(),
                to: self.linked.device.clone(),
                sealed,
            }
            .encode(),
            203,
        )?;
        assert_eq!(
            self.destination.saved.linking.pages[space].answer,
            Some((1, false))
        );
        assert!(self.destination.saved.linking.complete.contains(space));
        let snapshot = self.destination.update(Vec::new())?.snapshot;
        let resumed = Client::new(
            self.linked.clone(),
            HostSigner(Root::from_secret(&[106; 32])),
            &snapshot,
        )?;
        assert!(
            matches!(&resumed.logs[space].events()[1].body,EventBody::MessagePosted{text,..}if text=="Actual old message")
        );
        assert!(resumed.saved.linking.complete.contains(space));

        Ok(())
    }
    fn assert_oversize_refuses_completion_and_retry_loop(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let space = "sp_older";
        // JSON escaping may make one legitimate opened record larger than the
        // authenticated device-channel budget. Report it instead of retrying the
        // same empty boundary forever or claiming the history is complete.
        let mut large = self.author.sign_event(
            space,
            "msg_old",
            2,
            self.outer.seen.clone(),
            EventBody::MessagePosted {
                message: "msg_old".into(),
                text: "\n".repeat(600_000),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )?;
        large.seq = self.outer.seq;
        large.sealed_wire = Some(event_log::content_hash_of(&self.outer));
        let large = device_link::opened_history_event(&self.outer, large)
            .ok_or("authenticated large history event mismatch")?;
        let create = self.source.logs[space].events()[0].clone();
        self.source.logs.insert(
            space.into(),
            SpaceLog::from_events(space, vec![create.clone(), large])?,
        );
        self.destination = Client::new(
            self.linked.clone(),
            HostSigner(Root::from_secret(&[106; 32])),
            "",
        )?;
        self.destination.synced = true;
        self.destination.logs.insert(
            space.into(),
            SpaceLog::from_events(space, vec![create, self.outer.clone()])?,
        );
        let (req, sealed) = self.request_page(400)?;
        let update = self.destination.receive(
            &ServerFrame::DeviceMessage {
                from: self.source.account.device.clone(),
                to: self.linked.device.clone(),
                sealed,
            }
            .encode(),
            402,
        )?;
        assert!(!self.destination.saved.linking.complete.contains(space));
        let status = command(
            &mut self.destination,
            &json!({"op":"older_status","space_id":space,"request_id":req}),
            403,
        )?
        .result;
        assert_eq!(status["pending"], false);
        assert!(status["failure"]
            .as_str()
            .ok_or("missing authenticated test as_str value")?
            .contains("grande demais"));
        assert!(update.maintenance_ms.is_none_or(|deadline| deadline > 0));
        assert!(!self
            .destination
            .maintenance(10_000)?
            .frames
            .iter()
            .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .any(|frame| matches!(
                frame,
                ClientFrame::Req {
                    op: Op::SendDevice { .. },
                    ..
                }
            )));
        Ok(())
    }
}

fn device_ciphertext(
    update: &crate::client::Update,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    update
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .find_map(|frame| match frame {
            ClientFrame::Req {
                op: Op::SendDevice { sealed, .. },
                ..
            } => Some(sealed),
            _ => None,
        })
        .ok_or_else(|| "authenticated device reply missing".into())
}
