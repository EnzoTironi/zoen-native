//! Real signed Core and its existing index, without relay/provider/device QA.
use crate::agent_binary_documents::tests::{fixture, Fixture};
use types::{Annotation, EventBody, Privacy, Role, SpaceKind};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn post(f: &Fixture, space: &str, text: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(f.agent
        .lock()
        .append(
            space,
            &f.owner,
            EventBody::MessagePosted {
                message: types::new_id("history")?,
                text: text.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )?
        .client_id)
}

#[test]
fn deep_history_uses_real_message_and_author_refs_with_scoped_pagination() -> TestResult {
    let mut f = fixture()?;
    let first = post(
        &f,
        &f.space,
        "Café Paraty was agreed before the recent context",
    )?;
    let second = post(&f, &f.space, "Café Paraty second real message")?;
    for n in 0..25 {
        post(&f, &f.space, &format!("Recent unrelated message {n}"))?;
    }
    f.source = post(&f, &f.space, "Find the older travel agreement")?;
    let foreign = types::new_id("foreign_history")?;
    {
        let mut e = f.agent.lock();
        let me = e.me_id()?;
        e.append(
            &foreign,
            &me,
            EventBody::SpaceCreated {
                title: "Another conversation".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )?;
        e.append(
            &foreign,
            &me,
            EventBody::MemberAdded {
                identity: f.owner.clone(),
                role: Role::Member,
            },
        )?;
    }
    let foreign_id = post(&f, &foreign, "Café Paraty secret in another conversation")?;
    let before = counts(&f)?;
    let page =
        f.agent
            .agent_search_messages(&f.space, &f.source, &f.owner, "cafe paraty", None, 1)?;
    assert_eq!(page.hits.len(), 1);
    let cursor = page.next_cursor.as_deref().ok_or("missing next page")?;
    let next = f.agent.agent_search_messages(
        &f.space,
        &f.source,
        &f.owner,
        "cafe paraty",
        Some(cursor),
        1,
    )?;
    assert_eq!(next.hits.len(), 1);
    assert!(next.next_cursor.is_none());
    let found: std::collections::BTreeSet<_> = page
        .hits
        .iter()
        .chain(&next.hits)
        .map(|hit| {
            assert_eq!(hit.space_id.as_deref(), Some(f.space.as_str()));
            assert_eq!(
                hit.persona.as_ref().map(|p| p.id.as_str()),
                Some(f.owner.as_str())
            );
            assert_ne!(hit.ref_id, foreign_id);
            hit.ref_id.clone()
        })
        .collect();
    assert_eq!(found, std::collections::BTreeSet::from([first, second]));
    assert!(f
        .agent
        .agent_search_messages(&foreign, &f.source, &f.owner, "cafe", None, 1)
        .is_err());
    assert!(f
        .agent
        .agent_search_messages(&f.space, &f.source, &f.owner, "different", Some(cursor), 1)
        .is_err());
    assert_eq!(
        counts(&f)?[..5],
        before[..5],
        "search must not publish events, Requests, usage or outbox writes"
    );
    Ok(())
}

#[test]
fn source_deletion_member_removal_and_non_agent_callers_refuse_reads() -> TestResult {
    let f = fixture()?;
    assert!(f
        .owner_client
        .agent_search_messages(&f.space, &f.source, &f.owner, "Update", None, 1)
        .is_err());
    assert!(f
        .agent
        .agent_search_messages(&f.space, "missing", &f.owner, "Update", None, 1)
        .is_err());
    for limit in [0, 21, u32::MAX] {
        assert!(f
            .agent
            .agent_search_messages(&f.space, &f.source, &f.owner, "Update", None, limit)
            .is_err());
    }
    assert!(f
        .agent
        .agent_search_messages(&f.space, &f.source, &f.owner, &"x".repeat(513), None, 1)
        .is_err());
    let before = counts(&f)?;
    f.agent.lock().append(
        &f.space,
        &f.owner,
        EventBody::MessageAnnotated {
            target: f.source.clone(),
            note: Annotation::Delete,
        },
    )?;
    assert!(f
        .agent
        .agent_search_messages(&f.space, &f.source, &f.owner, "Update", None, 1)
        .is_err());
    // Separate genuine source: the previous deletion cannot mask a membership failure.
    let next_source = post(&f, &f.space, "Search again")?;
    assert!(f
        .agent
        .agent_search_messages(&f.space, &next_source, &f.owner, "Update", None, 1)
        .is_ok());
    let me = f.agent.account().ok_or("missing account")?.identity_id;
    f.agent.lock().append(
        &f.space,
        &me,
        EventBody::MemberRemoved {
            identity: f.owner.clone(),
        },
    )?;
    assert!(f
        .agent
        .agent_search_messages(&f.space, &next_source, &f.owner, "Update", None, 1)
        .is_err());
    let after = counts(&f)?;
    assert!(
        after[0] > before[0],
        "only deliberate fixture events were appended"
    );
    Ok(())
}

fn counts(f: &Fixture) -> Result<Vec<i64>, Box<dyn std::error::Error>> {
    let engine = f.agent.lock();
    ["events", "media", "media_keys", "blob_uploads", "outbox"]
        .iter()
        .map(|table| {
            engine
                .store
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
        })
        .collect::<Result<Vec<_>, rusqlite::Error>>()
        .map_err(Into::into)
}
