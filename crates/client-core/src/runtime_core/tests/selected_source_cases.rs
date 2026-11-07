//! Selection reads the exact authenticated message without touching discovery.
use super::*;
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
fn message_id(f: &Fixture, hash: &str) -> std::result::Result<String, Box<dyn std::error::Error>> {
    let event = f
        .core
        .engine
        .logs
        .get(SPACE)
        .and_then(|log| log.events().iter().find(|event| event.hash == hash))
        .ok_or("source")?;
    match &event.body {
        EventBody::MessagePosted { .. } => Ok(event.client_id.clone()),
        _ => Err("message".into()),
    }
}
#[test]
fn selected_native_message_preserves_scan_cursor_and_exact_identity() -> TestResult {
    let mut f = Fixture::new()?;
    f.grant("selected-source-grant", None)?;
    let first = f.message("First source")?;
    let second = f.message("Selected source")?;
    let id = message_id(&f, &second)?;
    let business_id = match &f.core.engine.logs[SPACE]
        .events()
        .iter()
        .find(|event| event.hash == second)
        .ok_or("source")?
        .body
    {
        EventBody::MessagePosted { message, .. } => message.clone(),
        _ => return Err("message".into()),
    };
    assert_ne!(id, business_id);
    let sha = crate::agent_page_tools::digest(b"Selected source");
    let now = crate::engine::now_ms();
    let before = f.core.image()?;
    let selected = f
        .core
        .reply_for_message(SPACE, &id, &f.owner_profile.id, &sha, now)?;
    assert_eq!(selected.intent().trigger(), second);
    assert!(f
        .core
        .reply_for_message(SPACE, &business_id, &f.owner_profile.id, &sha, now)
        .is_err());
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    let mut cold = DeviceCore::restore(f.core.image()?, f.credential())?;
    assert_eq!(
        cold.reply_for_message(SPACE, &id, &f.owner_profile.id, &sha, now)?
            .intent(),
        selected.intent()
    );
    for (space, message, author, digest) in [
        (
            SPACE,
            id.as_str(),
            f.owner_profile.id.as_str(),
            "00".repeat(32),
        ),
        (SPACE, id.as_str(), f.agent_profile.id.as_str(), sha.clone()),
        (
            "wrong-space",
            id.as_str(),
            f.owner_profile.id.as_str(),
            sha.clone(),
        ),
        (
            SPACE,
            &id[..id.len() - 1],
            f.owner_profile.id.as_str(),
            sha.clone(),
        ),
    ] {
        assert!(cold
            .reply_for_message(space, message, author, &digest, now)
            .is_err());
    }
    let discovery = cold.discover_replies(SPACE, now)?;
    assert!(discovery
        .replies
        .iter()
        .any(|facts| facts.intent().trigger() == first));
    assert!(discovery
        .replies
        .iter()
        .any(|facts| facts.intent().trigger() == second));
    f.encrypted(
        EventBody::GrantRevoked {
            grant: "selected-source-grant".into(),
        },
        now + 1,
    )?;
    assert!(f
        .core
        .reply_for_message(SPACE, &id, &f.owner_profile.id, &sha, now + 1)
        .is_err());
    Ok(())
}
