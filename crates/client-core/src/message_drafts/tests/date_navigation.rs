use super::*;
use crate::conversation_date::{select, MessageDateInput};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn calendar_query_is_read_only_owner_bound_and_uses_available_canonical_history() -> TestResult
{
    let f = fixture(":memory:");
    let message = f
        .engine
        .send_message(f.scope.space_id.clone(), "First available".into())?;
    let at = message.at_ms;
    let child = f.engine.send_reply(
        f.scope.space_id.clone(),
        "Thread only".into(),
        message.id.clone(),
        true,
    )?;
    let draft = save(&f, "0", "Keep my private draft", None);
    let before = counts(&f.engine);
    let request = serde_json::json!({"op":"message_date_target", "scope": {
        "expected_identity_id":f.scope.expected_identity_id,
        "expected_device_id":f.scope.expected_device_id,
        "space_id":f.scope.space_id,"thread_root_id":null
    },"start_ms":at.to_string(),"end_ms":(child.at_ms.max(at)+1).to_string()});
    let result = f.engine.client_request(request.to_string()).await?;
    assert_eq!(
        serde_json::from_str::<Option<String>>(&result)?,
        Some(message.id.clone())
    );
    assert_eq!(counts(&f.engine), before);
    assert_eq!(read(&f), draft);
    for field in ["expected_identity_id", "expected_device_id", "space_id"] {
        let mut changed = request.clone();
        changed["scope"][field] = "f".repeat(64).into();
        assert!(f.engine.client_request(changed.to_string()).await.is_err());
    }
    let mut unknown = request.clone();
    unknown["unrecognized"] = true.into();
    assert!(f.engine.client_request(unknown.to_string()).await.is_err());
    f.engine
        .delete_message(f.scope.space_id.clone(), message.id)?;
    assert_eq!(
        serde_json::from_str::<Option<String>>(
            &f.engine.client_request(request.to_string()).await?
        )?,
        None
    );
    assert_eq!(read(&f), draft);
    Ok(())
}

#[test]
fn calendar_boundaries_ignore_threads_deleted_and_service_entries_without_ordering_by_page(
) -> TestResult {
    let f = fixture(":memory:");
    let template = f
        .engine
        .send_message(f.scope.space_id.clone(), "A".into())?;
    let row = |id: &str, at: i64| {
        let mut copy = template.clone();
        copy.id = id.into();
        copy.at_ms = at;
        copy
    };
    let mut thread = row("thread", 100);
    thread.in_thread = Some(template.id.clone());
    let mut deleted = row("deleted", 101);
    deleted.deleted = true;
    let mut service = row("service", 102);
    service.kind = crate::EntryKind::System {
        text: "changed".into(),
    };
    assert_eq!(
        select(
            vec![
                row("after", 200),
                row("last", 199),
                row("before", 99),
                thread,
                deleted,
                service,
                row("first", 103)
            ],
            100,
            200
        ),
        Some("first".into())
    );
    assert_eq!(select(vec![row("boundary", 200)], 100, 200), None);
    Ok(())
}

#[test]
fn calendar_intervals_accept_dst_days_and_refuse_unbounded_or_noncanonical_ranges() -> TestResult {
    let f = fixture(":memory:");
    let mut input = MessageDateInput {
        scope: f.scope.clone(),
        start_ms: "100".into(),
        end_ms: "200".into(),
    };
    for hours in [23, 24, 25, 26] {
        input.end_ms = (100 + hours * 3_600_000_i64).to_string();
        assert_eq!(f.engine.lock().message_date_target(&input)?, None);
    }
    for (start, end) in [
        ("-1", "100"),
        ("0100", "200"),
        ("100", "100"),
        ("200", "100"),
        ("0", "93600001"),
        ("NaN", "200"),
        ("0", "9223372036854775808"),
    ] {
        input.start_ms = start.into();
        input.end_ms = end.into();
        assert!(f.engine.lock().message_date_target(&input).is_err());
    }
    Ok(())
}
