//! A genuine signed request remains scoped after an old inert view is restored.
use super::source_scope_tests::scope;
use super::tests::{counts, fixture};
use super::*;

#[test]
fn restored_inert_request_still_cannot_authorize_the_wrong_agent_or_share_anything(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    scope(&mut f, |r| {
        let a = r.artifact.as_mut().unwrap();
        a.agent_id = Some("other_operator".into());
        a.intent_sha256 = Some("11".repeat(32));
    })?;
    let directory = std::env::temp_dir().join(types::new_id("scoped_cohort")?);
    std::fs::create_dir(&directory)?;
    let path = directory.join("old-store.sqlite");
    let raw;
    {
        let e = f.agent.lock();
        let mut old = e
            .store
            .events(&f.space)?
            .into_iter()
            .find(|event| event.client_id == f.source)
            .ok_or("signed source")?;
        raw = old.content.clone();
        event_log::verify_author(&old)?;
        old.body = types::EventBody::Unsupported {
            kind: "AppEditRequested".into(),
        };
        e.store.conn().execute(
            "UPDATE events SET json=?1 WHERE space=?2 AND seq=?3",
            rusqlite::params![serde_json::to_string(&old)?, f.space, old.seq],
        )?;
        e.store
            .conn()
            .execute("VACUUM INTO ?1", [path.to_str().ok_or("fixture path")?])?;
    }
    let cold = Client::open(path.to_str().ok_or("fixture path")?.into(), "en".into())?;
    assert!(cold.unlock(f.agent_vault.clone())?);
    assert!(cold.verify_log(f.space.clone()).valid);
    {
        let e = cold.lock();
        let request = e.logs[&f.space]
            .events()
            .iter()
            .find(|event| event.client_id == f.source)
            .ok_or("restored source")?;
        assert_eq!(request.content, raw);
        assert!(matches!(
            request.body,
            types::EventBody::AppEditRequested { .. }
        ));
    }
    let original_agent = std::mem::replace(&mut f.agent, cold.clone());
    let before = counts(&f);
    assert!(cold
        .agent_artifact_request(&f.space, &f.source, &f.owner)
        .is_err());
    assert!(cold
        .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
        .is_err());
    assert!(cold
        .agent_artifact_reply(&f.space, &f.source, &f.owner, "Wrong operator")
        .is_err());
    assert_eq!(
        cold.app_binary_read(f.base.item_id.clone(), None, None)?
            .sha256,
        f.base.sha256
    );
    assert_eq!(counts(&f), before);
    drop(original_agent);
    drop(f);
    drop(cold);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
