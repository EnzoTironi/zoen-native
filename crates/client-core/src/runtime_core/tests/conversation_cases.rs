//! Managed execution retains normal conversation semantics without a second Engine.
use super::*;
type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn native_conversation_old_context_cannot_permanently_block_new_requests() -> TestResult {
    let mut f = Fixture::new()?;
    f.grant("bounded-conversation", None)?;
    f.message(&"a".repeat(24_000))?;
    let oversized = f.message(&"b".repeat(16_000))?;
    assert!(matches!(
        f.core.reply(SPACE, &oversized, 400),
        Err(NativeError::Bounds)
    ));
    let mut latest = String::new();
    for index in 0..20 {
        latest = f.message(&format!("Small request {index}"))?;
    }
    let facts = f.core.reply(SPACE, &latest, 400)?;
    assert_eq!(facts.messages().len(), 17);
    assert_eq!(facts.messages()[0].text(), "Small request 3");
    assert_eq!(facts.messages()[16].text(), "Small request 19");
    assert!(!facts.text().contains("aaaa"));
    let scan = f.core.discover_replies(SPACE, 400)?;
    assert!(scan.complete);
    assert!(scan
        .replies
        .iter()
        .any(|reply| reply.intent().trigger() == latest));
    assert!(scan
        .replies
        .iter()
        .all(|reply| reply.intent().trigger() != oversized));
    assert!(f.core.engine.logs[SPACE]
        .events()
        .iter()
        .any(|event| event.hash == oversized));
    Ok(())
}

fn id(f: &Fixture, hash: &str) -> TestResult<String> {
    Ok(f.owner.logs[SPACE]
        .events()
        .iter()
        .find(|event| event.hash == hash)
        .ok_or("source event")?
        .client_id
        .clone())
}

fn post(f: &mut Fixture, text: &str, reply: types::ReplyRef) -> TestResult<String> {
    f.encrypted(
        EventBody::MessagePosted {
            message: new_id("thread")?,
            text: text.into(),
            attaches: None,
            reply: Some(reply),
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
        350,
    )
}

#[test]
fn native_conversation_thread_keeps_context_exact_source_and_cold_receipt() -> TestResult {
    let mut f = Fixture::new()?;
    f.grant("thread-authority", None)?;
    let root = f.message("First discussion")?;
    let root_id = id(&f, &root)?;
    let other = f.message("Other discussion")?;
    let other_id = id(&f, &other)?;
    post(
        &mut f,
        "Other thread content",
        types::ReplyRef::thread(other_id),
    )?;
    let source = post(&mut f, "Exact question", types::ReplyRef::thread(&root_id))?;
    let source_id = id(&f, &source)?;
    let facts = f.core.reply(SPACE, &source, 400)?;
    assert_eq!(facts.intent().profile(), PROFILE);
    assert_eq!(
        facts
            .messages()
            .iter()
            .map(|entry| entry.source())
            .collect::<Vec<_>>(),
        vec![root.as_str(), source.as_str()]
    );
    let original = facts.intent().clone();
    let prepared = f.core.prepare_reply(&original, "Exact answer", 450)?;
    let envelope = prepared.envelope().clone();
    let client_id = envelope.client_id().to_owned();
    f.core = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec())?,
        f.credential(),
    )?;
    let frame = ordered(&f.owner, envelope.clone())?;
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    let received = f
        .owner
        .timeline(SPACE)?
        .into_iter()
        .find(|entry| entry.id == client_id)
        .ok_or("owner received answer")?;
    assert_eq!(received.in_thread.as_deref(), Some(root_id.as_str()));
    assert_eq!(
        received.reply_to.as_ref().ok_or("source quote")?.id,
        source_id
    );
    let receipt = f
        .core
        .receive_reply(&original, &client_id, vec![frame.clone()])?;
    assert_eq!(receipt.accepted().hash, frame.hash);
    let wire = ClientFrame::Publish { env: envelope }.encode();
    f.core = DeviceCore::restore(
        NativeImage::decode_bounded(receipt.image().as_bytes().to_vec())?,
        f.credential(),
    )?;
    let retained = f.core.retained_reply(&original, &client_id)?;
    assert_eq!(
        ClientFrame::Publish {
            env: retained.envelope().clone()
        }
        .encode(),
        wire
    );
    let mut nested = types::ReplyRef::thread(&root_id);
    nested.source = Some(client_id.clone());
    let next = post(&mut f, "Next question", nested)?;
    let facts = f.core.reply(SPACE, &next, 500)?;
    assert_eq!(
        facts
            .messages()
            .iter()
            .map(|entry| entry.text())
            .collect::<Vec<_>>(),
        vec![
            "First discussion",
            "Exact question",
            "Exact answer",
            "Next question"
        ]
    );
    let main = f.message("Back in main")?;
    let facts = f.core.reply(SPACE, &main, 550)?;
    assert_eq!(
        facts
            .messages()
            .iter()
            .map(|entry| entry.text())
            .collect::<Vec<_>>(),
        vec!["First discussion", "Other discussion", "Back in main"]
    );
    Ok(())
}

#[test]
fn native_conversation_legacy_intent_retains_v1_wire_after_cold_restore() -> TestResult {
    let mut f = Fixture::new()?;
    f.grant("legacy-authority", None)?;
    let source = f.message("Legacy request")?;
    let mut original = f.core.reply(SPACE, &source, 400)?.intent().clone();
    original.profile = LEGACY_PROFILE.into();
    let encoded = serde_json::to_vec(&original)?;
    let original: ReplyIntent = serde_json::from_slice(&encoded)?;
    let prepared = f.core.prepare_reply(&original, "Legacy answer", 450)?;
    let client_id = prepared.envelope().client_id().to_owned();
    let reference_key = super::super::reference::reference_key(&client_id);
    let reference = f
        .core
        .engine
        .store
        .meta(&reference_key)?
        .ok_or("legacy reference")?;
    let value: serde_json::Value = serde_json::from_str(&reference)?;
    assert_eq!(value["version"], 1);
    assert!(value.get("reply").is_none());
    let frame = ordered(&f.owner, prepared.envelope().clone())?;
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    let event = f.owner.logs[SPACE]
        .events()
        .last()
        .ok_or("received legacy")?;
    assert!(
        matches!(&event.body, EventBody::MessagePosted { reply: None, text, .. } if text == "Legacy answer")
    );
    f.core = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec())?,
        f.credential(),
    )?;
    f.core.receive_reply(&original, &client_id, vec![frame])?;
    assert_eq!(
        f.core.engine.store.meta(&reference_key)?.as_deref(),
        Some(reference.as_str())
    );
    Ok(())
}

#[test]
fn native_conversation_cannot_attribute_a_source_to_a_different_thread() -> TestResult {
    let mut f = Fixture::new()?;
    f.grant("thread-source-authority", None)?;
    let root = f.message("Root")?;
    let root_id = id(&f, &root)?;
    let other = f.message("Other")?;
    let other_id = id(&f, &other)?;
    let child = post(&mut f, "Other question", types::ReplyRef::thread(other_id))?;
    let mut forged = types::ReplyRef::thread(root_id);
    forged.source = Some(id(&f, &child)?);
    let author = f.owner.net.author.clone().ok_or("owner author")?;
    let client_id = new_id("wrong-thread")?;
    let head = f.owner.logs[SPACE].head();
    let event = author.sign_event(
        SPACE,
        &client_id,
        350,
        head.clone(),
        EventBody::MessagePosted {
            message: new_id("wrong-message")?,
            text: "Wrong source".into(),
            attaches: None,
            reply: Some(forged),
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
    )?;
    let data = f.owner.device()?.seal(
        SPACE,
        &InnerEvent {
            content: event.content,
            sig: event.sig,
        }
        .encode(),
    )?;
    let envelope = Envelope::sealed(
        &author,
        SPACE,
        &client_id,
        350,
        head.as_ref(),
        Sealed::new(SealedKind::Application, ::mls::SUITE_ID, data),
    )?;
    envelope.verify()?;
    let frame = ordered(&f.owner, envelope)?;
    let source = frame.hash.clone();
    let received = f.core.ingest(vec![frame]);
    assert!(received.is_err() || f.core.reply(SPACE, &source, 400).is_err());
    assert_eq!(f.core.engine.store.outbox_len()?, 0);
    Ok(())
}
