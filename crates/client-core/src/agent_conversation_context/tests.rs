use crate::agent_binary_documents::tests::{fixture, Fixture};
use types::{Annotation, EventBody};

mod threads;

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn post(f: &Fixture, text: &str) -> std::result::Result<String, crate::CoreError> {
    Ok(f.agent
        .lock()
        .append(
            &f.space,
            &f.owner,
            EventBody::MessagePosted {
                message: types::new_id("context")?,
                text: text.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
        )?
        .client_id)
}

fn annotate(f: &Fixture, source: &str, note: Annotation) -> Result {
    f.agent.lock().append(
        &f.space,
        &f.owner,
        EventBody::MessageAnnotated {
            target: source.into(),
            note,
        },
    )?;
    Ok(())
}

#[test]
fn context_is_native_bounded_ordered_and_ignores_later_messages() -> Result {
    let f = fixture()?;
    let mut ids = Vec::new();
    for n in 0..24 {
        ids.push(post(&f, &format!("Context {n}"))?);
    }
    annotate(&f, &ids[23], Annotation::Delete)?;
    annotate(
        &f,
        &ids[22],
        Annotation::Edit {
            text: "Latest edit".into(),
        },
    )?;
    let source = post(&f, "Answer this")?;
    post(&f, "Not part of this turn")?;
    let context = f
        .agent
        .agent_conversation_context(&f.space, &source, &f.owner)?;
    assert_eq!(context.history().len(), 16);
    assert_eq!(context.history()[0].id(), ids[7]);
    assert_eq!(context.history()[15].text(), "Latest edit");
    assert_eq!(context.source().text(), "Answer this");
    assert!(!context.scoped());
    f.agent.recheck_agent_conversation_context(&context)?;
    post(&f, "Another unrelated later message")?;
    f.agent.recheck_agent_conversation_context(&context)?;
    annotate(&f, &ids[22], Annotation::Delete)?;
    // An admitted history snapshot does not restart inference when history changes.
    f.agent.recheck_agent_conversation_context(&context)?;
    Ok(())
}

#[test]
fn source_revision_and_membership_are_rechecked_after_inference() -> Result {
    let f = fixture()?;
    let source = post(&f, "Original")?;
    let context = f
        .agent
        .agent_conversation_context(&f.space, &source, &f.owner)?;
    annotate(
        &f,
        &source,
        Annotation::Edit {
            text: "Changed".into(),
        },
    )?;
    annotate(
        &f,
        &source,
        Annotation::Edit {
            text: "Original".into(),
        },
    )?;
    assert!(f
        .agent
        .recheck_agent_conversation_context(&context)
        .is_err());
    assert!(f
        .agent
        .agent_conversation_reply(&context, "Obsolete answer")
        .is_err());
    let current = f
        .agent
        .agent_conversation_context(&f.space, &source, &f.owner)?;
    let me = f.agent.account().ok_or("account")?.identity_id;
    f.agent.lock().append(
        &f.space,
        &me,
        EventBody::MemberRemoved {
            identity: f.owner.clone(),
        },
    )?;
    assert!(f
        .agent
        .recheck_agent_conversation_context(&current)
        .is_err());
    assert!(f
        .agent
        .agent_conversation_context(&f.space, &source, &f.owner)
        .is_err());
    Ok(())
}

#[test]
fn deleted_source_wrong_author_and_person_client_cannot_supply_context() -> Result {
    let f = fixture()?;
    let source = post(&f, "Original")?;
    assert!(f
        .owner_client
        .agent_conversation_context(&f.space, &source, &f.owner)
        .is_err());
    assert!(f
        .agent
        .agent_conversation_context(&f.space, &source, "other")
        .is_err());
    let context = f
        .agent
        .agent_conversation_context(&f.space, &source, &f.owner)?;
    annotate(&f, &source, Annotation::Delete)?;
    assert!(f
        .agent
        .recheck_agent_conversation_context(&context)
        .is_err());
    assert!(f
        .agent
        .agent_conversation_context(&f.space, &source, &f.owner)
        .is_err());
    Ok(())
}

#[test]
fn scoped_source_never_imports_unrelated_history() -> Result {
    use sha2::{Digest, Sha256};
    let f = fixture()?;
    post(&f, "Unrelated private context")?;
    let mut e = f.agent.lock();
    let root = e
        .item_discussion(&f.space, &f.base.item_id)?
        .root
        .ok_or("root")?
        .id;
    let text = "Edit this document only";
    let mut reply = types::ReplyRef::thread(root);
    reply.artifact = Some(types::ArtifactRequest {
        item_id: f.base.item_id.clone(),
        version: f.base.version,
        sha256: f.base.sha256.clone(),
        request_id: types::new_id("context_request")?,
        text_sha256: hex::encode(Sha256::digest(text.as_bytes())),
        agent_id: None,
        intent_sha256: None,
    });
    let source = e
        .append(
            &f.space,
            &f.owner,
            EventBody::AppEditRequested {
                message: types::new_id("context")?,
                text: text.into(),
                reply,
            },
        )?
        .client_id;
    drop(e);
    let context = f
        .agent
        .agent_conversation_context(&f.space, &source, &f.owner)?;
    assert!(context.scoped());
    assert!(context.history().is_empty());
    assert!(f
        .agent
        .agent_conversation_reply(&context, "Use scoped port")
        .is_err());
    f.agent.recheck_agent_conversation_context(&context)?;
    Ok(())
}

#[test]
fn current_context_publishes_a_real_linked_native_reply() -> Result {
    let f = fixture()?;
    let source = post(&f, "Answer here")?;
    let context = f
        .agent
        .agent_conversation_context(&f.space, &source, &f.owner)?;
    let reply = f.agent.agent_conversation_reply(&context, "Native reply")?;
    let recovered = f.agent.agent_conversation_reply(&context, "Native reply")?;
    assert_eq!(recovered.id, reply.id);
    assert!(f
        .agent
        .agent_conversation_reply(&context, "Different reply")
        .is_err());
    assert_eq!(
        reply.author.id,
        f.agent.account().ok_or("account")?.identity_id
    );
    assert_eq!(reply.reply_to.ok_or("reply reference")?.id, source);
    assert!(matches!(reply.kind, crate::EntryKind::Message { text, .. } if text == "Native reply"));
    Ok(())
}
