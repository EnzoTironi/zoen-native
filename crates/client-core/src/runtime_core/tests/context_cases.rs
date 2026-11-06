//! Role and source extraction uses the same signed MLS history as replies.
use super::*;

#[test]
fn authenticated_context_keeps_roles_sources_window_and_grant_reset(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    f.grant("context-authority", None)?;
    let first = f.message("Owner first")?;
    let intent = f.core.reply(SPACE, &first, 400)?.intent().clone();
    let prepared = f.core.prepare_reply(&intent, "Agent reply", 450)?;
    let frame = ordered(&f.owner, prepared.envelope().clone())?;
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    let agent_source = frame.hash.clone();
    f.core.ingest(vec![frame])?;
    let last = f.message("Owner next")?;
    let facts = f.core.reply(SPACE, &last, 500)?;
    let entries = facts.messages();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].author(), f.owner_profile.id);
    assert_eq!(entries[0].source(), first);
    assert_eq!(entries[0].text(), "Owner first");
    assert_eq!(entries[1].author(), f.agent_profile.id);
    assert_eq!(entries[1].source(), agent_source);
    assert_eq!(entries[1].text(), "Agent reply");
    assert_eq!(entries[2].source(), last);
    assert!(!format!("{:?}", entries[1]).contains("Agent reply"));
    let mut sources = Vec::new();
    for index in 0..20 {
        sources.push(f.message(&format!("Message {index}"))?);
    }
    let trigger = sources.last().ok_or("trigger")?;
    let facts = f.core.reply(SPACE, trigger, 600)?;
    assert_eq!(facts.messages().len(), 17);
    assert_eq!(facts.messages()[0].source(), sources[3]);
    assert_eq!(facts.messages()[16].source(), trigger);
    let cold = DeviceCore::restore(f.core.image()?, f.credential())?;
    assert_eq!(
        cold.reply(SPACE, trigger, 600)?.messages()[0].source(),
        sources[3]
    );
    f.grant("new-context-baseline", None)?;
    let trigger = f.message("New baseline")?;
    let facts = f.core.reply(SPACE, &trigger, 600)?;
    assert_eq!(facts.messages().len(), 1);
    assert_eq!(facts.messages()[0].text(), "New baseline");
    Ok(())
}
