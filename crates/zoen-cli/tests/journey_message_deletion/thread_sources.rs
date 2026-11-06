//! Two real MLS clients receive the exact source and root, then reopen their stores.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_thread_reply_keeps_exact_source_across_delivery_and_cold_reopen() -> TestResult {
    let (w, space) = enrolled("thread_source_peers").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let root = alice
        .0
        .send_message(space.clone(), "Root discussion".into())?;
    received(&bob.0, &space, |rows| {
        rows.iter().any(|entry| entry.id == root.id)
    })
    .await?;
    let question = alice.0.send_reply(
        space.clone(),
        "Question inside the thread".into(),
        root.id.clone(),
        true,
    )?;
    received(&bob.0, &space, |rows| {
        rows.iter().any(|entry| entry.id == question.id)
    })
    .await?;
    let answer = bob.0.send_reply(
        space.clone(),
        "Answer to that exact question".into(),
        question.id.clone(),
        true,
    )?;
    received(&alice.0, &space, |rows| {
        rows.iter().any(|entry| entry.id == answer.id)
    })
    .await?;
    alice.0.wait_until_settled(20_000).await?;
    bob.0.wait_until_settled(20_000).await?;
    drop(alice);
    drop(bob);
    for person in ["alice", "bob"] {
        let reopened = Open::offline(&w, person)?;
        let timeline = reopened.0.timeline(space.clone())?;
        let received = timeline
            .iter()
            .find(|entry| entry.id == answer.id)
            .ok_or("answer absent after cold reopen")?;
        assert_eq!(received.in_thread.as_deref(), Some(root.id.as_str()));
        assert_eq!(
            received.reply_to.as_ref().ok_or("exact question")?.id,
            question.id
        );
        assert_eq!(
            received.reply_to.as_ref().ok_or("quote")?.text,
            "Question inside the thread"
        );
        assert!(
            matches!(&received.kind, EntryKind::Message { text, .. } if text == "Answer to that exact question")
        );
        let thread = reopened.0.thread(space.clone(), root.id.clone())?;
        assert_eq!(
            thread
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec![root.id.as_str(), question.id.as_str(), answer.id.as_str()]
        );
        assert_eq!(
            timeline
                .iter()
                .filter(|entry| entry.id == answer.id)
                .count(),
            1
        );
    }
    Ok(())
}
