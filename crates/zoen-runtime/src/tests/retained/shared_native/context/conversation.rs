//! Real financial capture and relay publication preserve the same native thread.
use super::*;

async fn source_id(f: &Fixture, hash: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(f.log
        .read(&f.space, 1, 256)
        .await?
        .into_iter()
        .find(|event| event.hash == hash)
        .ok_or("source")?
        .env
        .client_id()
        .to_owned())
}

async fn question(
    f: &mut Fixture,
    root: &str,
    text: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    f.encrypted(EventBody::MessagePosted {
        message: types::new_id("thread")?,
        text: text.into(),
        attaches: None,
        reply: Some(types::ReplyRef::thread(root)),
        media: None,
        forwarded: false,
        forwarded_author: None,
    })
    .await
}

async fn discover(f: &Fixture) -> Result<Vec<crate::ReplyRun>, Box<dyn std::error::Error>> {
    let mut runs = Vec::new();
    for _ in 0..16 {
        let sync = f.sync(false).await?;
        runs.extend(sync.runs);
        if sync.caught_up && sync.discovery_complete {
            return Ok(runs);
        }
    }
    Err("bounded fixture source did not finish discovery".into())
}

fn user_context(request: &serde_json::Value) -> Result<Vec<&str>, Box<dyn std::error::Error>> {
    let messages = request["messages"].as_array().ok_or("messages")?;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(text(&messages[0])?, definition(1).system_prompt());
    assert_eq!(messages[1]["role"], "user");
    // Rig preserves consecutive same-role turns as ordered content blocks.
    messages[1]["content"]
        .as_array()
        .ok_or("user blocks")?
        .iter()
        .map(|block| {
            assert_eq!(block["type"], "text");
            block["text"]
                .as_str()
                .ok_or_else(|| "user block text".into())
        })
        .collect()
}

pub(super) async fn run() -> TestResult {
    let mut f = Box::pin(claimed()).await?;
    f.grant("conversation-thread-authority").await?;
    let root = f.message("Travel discussion").await?;
    let root_id = source_id(&f, &root).await?;
    let other = f.message("Unrelated discussion").await?;
    let other_id = source_id(&f, &other).await?;
    assert_eq!(discover(&f).await?.len(), 2);
    question(&mut f, &other_id, "Unrelated thread content").await?;
    assert_eq!(discover(&f).await?.len(), 1);
    let source = question(&mut f, &root_id, "Question in this thread").await?;
    let source_id = source_id(&f, &source).await?;
    let mut runs = discover(&f).await?;
    assert_eq!(runs.len(), 1);
    let run = runs.pop().ok_or("thread run")?;
    output_route::configured(&f, true).await?;
    let relay = Relay::start(&f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let original = Box::pin(f.world.runtime.run_model(&run.run)).await?;
    {
        let requests = f.world.http.requests.lock().map_err(|_| "requests")?;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            user_context(&requests[0])?,
            vec!["Travel discussion", "Question in this thread"]
        );
    }
    // A cold executor resumes the paid original, not another model request.
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    assert_eq!(
        Box::pin(f.world.runtime.run_model(&run.run))
            .await?
            .financial,
        original.financial
    );
    let prepared = f.world.runtime.prepare_reply_output(&run.run).await?;
    assert!(matches!(
        f.world
            .runtime
            .deliver_reply_output(&run.run, relay.target(&f, relay.address))
            .await?,
        crate::ReplyReceiptProgress::Accepted { .. }
    ));
    let entries = f.log.read(&f.space, 1, 256).await?;
    let answer = entries
        .iter()
        .find(|event| event.env.client_id() == prepared.client_id)
        .ok_or("actual relay answer")?;
    answer.env.verify()?;
    assert_eq!(answer.env.author(), f.agent.id);
    let actor = &f.world.agents[0];
    assert_eq!(answer.env.device(), actor.device.as_deref());
    assert_eq!(answer.env.cert, actor.cert);
    let leaf = mls::Leaf {
        identity: f.agent.id.clone(),
        device: actor.device.clone().ok_or("device")?,
        cert: actor.cert.clone().ok_or("certificate")?,
    };
    let (_, sealed) = answer.env.sealed_data().ok_or("sealed answer")?;
    let roster = std::collections::BTreeSet::from([f.owner.id.clone(), f.agent.id.clone()]);
    let Opened::Application { plaintext, from } =
        f.device().open(&f.space, sealed, &roster, &leaf)?
    else {
        return Err("owner could not open the actual answer".into());
    };
    assert_eq!(from, leaf);
    let inner = InnerEvent::decode(&plaintext).ok_or("inner event")?;
    let clear = event_log::event_from_content(
        inner.content,
        inner.sig,
        answer.env.cert.clone(),
        answer.seq,
        answer.prev.clone(),
        answer.hash.clone(),
    )?;
    event_log::verify_author(&clear)?;
    assert!(
        matches!(clear.body, EventBody::MessagePosted { text: body, reply: Some(reply), .. }
        if body == crate::tests::BODY && reply.thread && reply.to == root_id
            && reply.source.as_deref() == Some(source_id.as_str()))
    );
    let accepted = answer.hash.clone();
    f.head = entries.last().map(|entry| Seen {
        seq: entry.seq,
        hash: entry.hash.clone(),
    });
    // A main-chat continuation must not import either thread's conversation.
    f.message("Continue in main chat").await?;
    let mut runs = discover(&f).await?;
    assert_eq!(runs.len(), 1);
    let next = runs.pop().ok_or("main run")?;
    Box::pin(f.world.runtime.run_model(&next.run)).await?;
    {
        let requests = f.world.http.requests.lock().map_err(|_| "requests")?;
        assert_eq!(requests.len(), 2);
        assert_eq!(
            user_context(&requests[1])?,
            vec![
                "Travel discussion",
                "Unrelated discussion",
                "Continue in main chat"
            ]
        );
    }
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    assert!(
        matches!(f.world.runtime.deliver_reply_output(&run.run, relay.target(&f, relay.address)).await?,
        crate::ReplyReceiptProgress::Accepted { accepted: position, .. } if position.hash == accepted)
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 2);
    assert_eq!(f.world.balance().await.spent_units, 12);
    assert_eq!(
        f.log
            .read(&f.space, 1, 256)
            .await?
            .iter()
            .filter(|entry| entry.env.client_id() == prepared.client_id)
            .count(),
        1
    );
    relay.stop().await;
    f.finish().await;
    println!("native conversation: scoped thread input, original paid capture, cold executor, exact source/root at owner MLS, main context isolation and one charge per inference PASS");
    Ok(())
}
