//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn profile(root: &Signer, kind: IdentityKind) -> Identity {
    Identity {
        id: root.id(),
        kind,
        name: "Certified test principal".into(),
        handle: "certified".into(),
        tint_hex: "#4F7CFF".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
        owner_proof: None,
    }
}

pub(super) fn ordered(
    engine: &Engine,
    envelope: Envelope,
) -> std::result::Result<Sequenced, Box<dyn std::error::Error>> {
    let log = engine.logs.get(envelope.space());
    let seq = log.map(SpaceLog::next_seq).unwrap_or(0);
    let prev = log
        .and_then(|l| l.events().last())
        .map(|e| e.hash.clone())
        .unwrap_or_else(|| GENESIS_PREV.into());
    Ok(Sequenced {
        seq,
        hash: chain_hash(envelope.space(), seq, &prev, &envelope.wire_hash())?,
        prev,
        env: envelope,
    })
}

pub(super) fn colliding_native_reply(
    fixture: &Fixture,
    prepared: &PreparedReply,
) -> std::result::Result<Envelope, Box<dyn std::error::Error>> {
    let fork = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let original = prepared.envelope();
    let author = fork.engine.net.author.as_ref().unwrap();
    let event = author.sign_event(
        SPACE,
        original.client_id(),
        original.at_ms(),
        original.seen(),
        EventBody::MessagePosted {
            message: new_id("different-message")?,
            text: "Different native result".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
    )?;
    let bytes = fork
        .engine
        .device()
        .unwrap()
        .seal(
            SPACE,
            &InnerEvent {
                content: event.content,
                sig: event.sig,
            }
            .encode(),
        )
        .unwrap();
    Ok(Envelope::sealed(
        author,
        SPACE,
        original.client_id(),
        original.at_ms(),
        original.seen().as_ref(),
        Sealed::new(SealedKind::Application, ::mls::SUITE_ID, bytes),
    )?)
}

pub(super) fn receipt_source_message(
    fixture: &mut Fixture,
    index: usize,
) -> std::result::Result<Sequenced, Box<dyn std::error::Error>> {
    let author = fixture.owner.net.author.clone().unwrap();
    fixture
        .owner
        .append_synced(
            &fixture.space,
            &author,
            600 + index as i64,
            EventBody::MessagePosted {
                message: new_id("prefix")?,
                text: format!("Historical position {index}"),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
        )
        .unwrap();
    fixture.owner.mls_seal_outbox().unwrap();
    let frame = ordered(&fixture.owner, fixture.owner.outbox_envelopes().remove(0))?;
    assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Confirmed);
    Ok(frame)
}

pub(super) fn sealed_frame(
    author: &Author,
    client_id: &str,
    seen: Option<&Seen>,
) -> std::result::Result<Envelope, Box<dyn std::error::Error>> {
    Ok(Envelope::sealed(
        author,
        SPACE,
        client_id,
        1_700_000_000_500,
        seen,
        Sealed::new(SealedKind::Application, ::mls::SUITE_ID, vec![9, 9, 9]),
    )?)
}
