use super::*;

fn post_in(f: &Fixture, root: &str, text: &str) -> std::result::Result<String, crate::CoreError> {
    Ok(f.agent
        .lock()
        .append(
            &f.space,
            &f.owner,
            EventBody::MessagePosted {
                message: types::new_id("thread_source")?,
                text: text.into(),
                attaches: None,
                reply: Some(types::ReplyRef::thread(root)),
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
        )?
        .client_id)
}

#[test]
fn agent_answers_exact_thread_source_and_recovers_without_confusing_sibling_requests() -> Result {
    let f = fixture()?;
    let root = post(&f, "Travel discussion")?;
    let other = post(&f, "Another discussion")?;
    let first = post_in(&f, &root, "First question")?;
    post_in(&f, &other, "Unrelated thread")?;
    post(&f, "Unrelated main conversation")?;
    let context = f
        .agent
        .agent_conversation_context(&f.space, &first, &f.owner)?;
    assert_eq!(context.thread_root(), Some(root.as_str()));
    assert_eq!(
        context.history().iter().map(|m| m.id()).collect::<Vec<_>>(),
        vec![root.as_str()]
    );
    let answer = f.agent.agent_conversation_reply(&context, "First answer")?;
    assert_eq!(answer.in_thread.as_deref(), Some(root.as_str()));
    assert_eq!(answer.reply_to.as_ref().ok_or("exact source")?.id, first);
    f.agent.lock().reload()?;
    assert_eq!(
        f.agent
            .agent_conversation_reply(&context, "First answer")?
            .id,
        answer.id
    );

    let second = post_in(&f, &root, "Second question")?;
    let context = f
        .agent
        .agent_conversation_context(&f.space, &second, &f.owner)?;
    assert_eq!(
        context.history().iter().map(|m| m.id()).collect::<Vec<_>>(),
        vec![root.as_str(), first.as_str(), answer.id.as_str()]
    );
    let next = f
        .agent
        .agent_conversation_reply(&context, "Second answer")?;
    assert_ne!(next.id, answer.id);
    assert_eq!(next.in_thread.as_deref(), Some(root.as_str()));
    assert_eq!(next.reply_to.as_ref().ok_or("second source")?.id, second);
    assert_eq!(
        f.agent
            .agent_conversation_reply(&context, "Second answer")?
            .id,
        next.id
    );
    let main = post(&f, "Back in main")?;
    let context = f
        .agent
        .agent_conversation_context(&f.space, &main, &f.owner)?;
    assert!(context.thread_root().is_none());
    let threaded = [
        first.as_str(),
        second.as_str(),
        answer.id.as_str(),
        next.id.as_str(),
    ];
    assert!(context
        .history()
        .iter()
        .all(|entry| !threaded.contains(&entry.id())));
    Ok(())
}

#[test]
fn thread_source_cannot_attribute_a_known_message_from_another_thread() -> Result {
    let f = fixture()?;
    let root = post(&f, "First thread")?;
    let other = post(&f, "Other thread")?;
    let source = post_in(&f, &root, "Question")?;
    let mut reply = types::ReplyRef::thread(&root);
    reply.source = Some(source.clone());
    let e = f.agent.lock();
    assert!(crate::replies::thread_source::valid(
        &e.state, &f.space, &f.owner, &reply
    ));
    reply.to = other;
    assert!(!crate::replies::thread_source::valid(
        &e.state, &f.space, &f.owner, &reply
    ));
    reply.thread = false;
    assert!(!crate::replies::thread_source::valid(
        &e.state, &f.space, &f.owner, &reply
    ));
    Ok(())
}
