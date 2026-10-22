//! Shared synthetic MLS fixtures and delivery helpers.
use super::*;

pub(super) fn ordered(engine: &Engine, env: Envelope) -> Sequenced {
    let (seq, prev) = engine
        .logs
        .get(env.space())
        .map(|log| {
            (
                log.next_seq(),
                log.events()
                    .last()
                    .map(|e| e.hash.clone())
                    .unwrap_or_else(|| types::GENESIS_PREV.into()),
            )
        })
        .unwrap_or_else(|| (0, types::GENESIS_PREV.into()));
    Sequenced {
        seq,
        hash: chain_hash(env.space(), seq, &prev, &env.wire_hash()),
        prev,
        env,
    }
}

pub(super) fn seen(engine: &Engine, space: &str) -> Option<types::Seen> {
    engine
        .logs
        .get(space)
        .and_then(|log| log.events().last())
        .map(|e| types::Seen {
            seq: e.seq,
            hash: e.hash.clone(),
        })
}

pub(super) fn confirm_claim_handshakes(engine: &mut Engine) -> Vec<Sequenced> {
    for (sha, _) in engine.pending_uploads(100) {
        engine.upload_done(&sha);
    }
    let mut frames = Vec::new();
    // Confirmation may enqueue the epoch checkpoint; order it through the same log.
    while engine.outbox_len() > 0 {
        let pending = engine.store.outbox().unwrap();
        assert!(!pending.is_empty());
        for queued in pending {
            let frame = ordered(engine, Envelope::plain(&queued.event));
            assert_eq!(engine.ingest(frame.clone()), Ingest::Confirmed);
            frames.push(frame);
        }
        assert!(frames.len() < 10, "handshake confirmation did not settle");
    }
    frames
}

pub(super) fn claim_group(
    engine: &mut Engine,
    space: &str,
    peers: &mut [Engine],
) -> Vec<Sequenced> {
    claim_space(engine, space, SpaceKind::Group, peers)
}

pub(super) fn claim_space(
    engine: &mut Engine,
    space: &str,
    kind: SpaceKind,
    peers: &mut [Engine],
) -> Vec<Sequenced> {
    let author = engine.net.author.clone().unwrap();
    let mut bodies = vec![EventBody::SpaceCreated {
        title: space.into(),
        kind,
        privacy: Privacy::EndToEnd,
    }];
    for peer in peers.iter_mut() {
        peer.net.profiles.supported = true;
        let identity = peer.me.clone().unwrap();
        let (public, signed) = peer.agreement_to_publish().unwrap();
        engine.agreement_keys_arrived(
            vec![protocol::AgreementKeyRecord {
                identity: identity.clone(),
                public,
                signed,
            }],
            std::slice::from_ref(&identity),
        );
        bodies.push(EventBody::MemberAdded {
            identity,
            role: Role::Member,
        });
    }
    let mut frames = Vec::new();
    for (i, body) in bodies.into_iter().enumerate() {
        let env = Envelope::plain(&author.sign_event(
            space,
            &format!("{space}-{i}"),
            1,
            seen(engine, space),
            body,
        ));
        let frame = ordered(engine, env);
        assert_eq!(engine.ingest(frame.clone()), Ingest::Applied);
        frames.push(frame);
    }
    engine.create_mls_group(space).unwrap();
    frames
}

pub(super) fn claimed_records(
    peers: &mut [Engine],
    targets: &[String],
) -> Vec<protocol::KeyPackageRecord> {
    peers
        .iter_mut()
        .filter_map(|peer| {
            let identity = peer.me.clone().unwrap();
            if !targets.contains(&identity) {
                return None;
            }
            let device = peer.net.account.as_ref().unwrap().device.clone();
            let data = peer
                .device()
                .unwrap()
                .key_packages(1, false)
                .unwrap()
                .remove(0);
            Some(protocol::KeyPackageRecord {
                identity,
                device,
                data,
            })
        })
        .collect()
}

pub(super) fn relay_ready_to_peer(owner: &mut Engine, peer: &mut Engine) -> Vec<Sequenced> {
    let mut published = Vec::new();
    for _ in 0..10 {
        for (sha, _) in owner.pending_uploads(100) {
            owner.upload_done(&sha);
        }
        owner.mls_seal_outbox().unwrap();
        let envelopes = owner.outbox_envelopes();
        if envelopes.is_empty() {
            assert_eq!(
                owner.outbox_len(),
                0,
                "queued event did not become publishable"
            );
            return published;
        }
        for env in envelopes {
            let frame = ordered(owner, env);
            assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
            assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
            published.push(frame);
        }
    }
    panic!("canonical handshake/application publication did not settle");
}

pub(super) fn member(path: &str) -> (Engine, Author, [u8; 32]) {
    let mut engine = Engine::open(path).unwrap();
    let (_, device_secret, _) = engine
        .create_account("Ana", "ana_u18", "http://relay.test")
        .unwrap();
    let author = engine.net.author.clone().unwrap();
    (engine, author, device_secret)
}

pub(super) fn post(text: &str) -> EventBody {
    EventBody::MessagePosted {
        message: format!("m-{text}"),
        text: text.into(),
        attaches: None,
        reply: None,
        media: None,
        forwarded: false,
    }
}

pub(super) fn checkpoint(upto: types::Seen, digest: &str) -> EventBody {
    EventBody::Checkpoint {
        upto,
        epoch: 1,
        digest: digest.into(),
    }
}

pub(super) fn ingest_signed(engine: &mut Engine, author: &Author, space: &str, body: EventBody) {
    let seq = engine.next_seq(space);
    let event = author.sign_event(
        space,
        &format!("{space}-{seq}"),
        1_700_000_000_000 + seq as i64,
        seen(engine, space),
        body,
    );
    let got = engine.ingest(ordered(engine, Envelope::plain(&event)));
    assert!(
        matches!(got, Ingest::Applied | Ingest::Confirmed),
        "ingest {seq}: {got:?}"
    );
}

pub(super) fn start_space(engine: &mut Engine, author: &Author, space: &str) {
    ingest_signed(
        engine,
        author,
        space,
        EventBody::SpaceCreated {
            title: space.into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    );
}

pub(super) fn hash_at(engine: &Engine, space: &str, seq: u64) -> String {
    engine.logs[space].events()[seq as usize].hash.clone()
}

pub(super) fn integrity(engine: &Engine, space: &str) -> Option<String> {
    engine
        .state
        .spaces
        .get(space)
        .and_then(|s| s.integrity_error.clone())
}

pub(super) fn device_key(author: &Author) -> String {
    author
        .device
        .clone()
        .unwrap_or_else(|| author.identity.clone())
}

pub(super) fn add_member(engine: &mut Engine, owner: &Author, space: &str, peer: &Author) {
    ingest_signed(
        engine,
        owner,
        space,
        EventBody::MemberAdded {
            identity: peer.identity.clone(),
            role: Role::Member,
        },
    );
}

pub(super) fn pending(engine: &Engine, space: &str) -> BTreeMap<String, PendingPrefix> {
    engine
        .store
        .meta(&pending_prefix_meta(space))
        .unwrap()
        .map(|raw| serde_json::from_str(&raw).unwrap())
        .unwrap_or_default()
}

pub(super) fn claim(seq: u64, hash: &str) -> EventBody {
    checkpoint(
        types::Seen {
            seq,
            hash: hash.into(),
        },
        &"ab".repeat(32),
    )
}

pub(super) fn extend_until(engine: &mut Engine, author: &Author, space: &str, seq: u64) {
    while engine.next_seq(space) <= seq {
        let n = engine.next_seq(space);
        ingest_signed(engine, author, space, post(&format!("reach-{n}")));
    }
}

pub(super) fn deliver(engine: &mut Engine, event: Event) -> Ingest {
    engine.ingest(ordered(engine, Envelope::plain(&event)))
}

pub(super) fn pruned_frame(engine: &Engine, author: &Author, space: &str) -> Sequenced {
    let seen = engine.logs.get(space).and_then(SpaceLog::head);
    let seq = engine.next_seq(space);
    let sealed = Envelope::sealed(
        author,
        space,
        &format!("{space}-pruned-{seq}"),
        1_700_000_000_000 + seq as i64,
        seen.as_ref(),
        event_log::content::Sealed::new(
            event_log::content::SealedKind::Application,
            1,
            vec![7, 7, 7, 7],
        ),
    );
    ordered(
        engine,
        sealed.pruned().expect("a sealed event leaves a stub"),
    )
}
