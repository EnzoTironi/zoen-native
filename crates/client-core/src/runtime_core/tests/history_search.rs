use super::*;
use serde_json::json;

#[test]
fn native_history_search_pages_beyond_model_context_from_cold_images(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    f.grant("history-read", None)?;
    for number in 0..25 {
        f.message(&format!("Paraty itinerary memory {number}"))?;
    }
    let source = f.message("Find our older itinerary")?;
    let intent = f.core.reply(SPACE, &source, 400)?.intent().clone();
    assert_eq!(f.core.reply(SPACE, &source, 400)?.messages().len(), 17);
    let image = f.core.image()?;
    let page = DeviceCore::restore(
        NativeImage::decode_bounded(image.as_bytes().to_vec())?,
        f.credential(),
    )?
    .search_reply_history(
        &intent,
        &json!({"query":"Paraty itinerary","limit":20}),
        400,
    )?;
    assert_eq!(page.hits.len(), 20);
    assert!(page
        .hits
        .iter()
        .all(|hit| hit.space_id.as_deref() == Some(SPACE)));
    let cursor = page.next_cursor.ok_or("cursor")?;
    let next = DeviceCore::restore(
        NativeImage::decode_bounded(image.as_bytes().to_vec())?,
        f.credential(),
    )?
    .search_reply_history(
        &intent,
        &json!({"query":"Paraty itinerary","cursor":cursor,"limit":20}),
        400,
    )?;
    assert_eq!(next.hits.len(), 5);
    assert!(next.next_cursor.is_none());
    let ids: std::collections::BTreeSet<_> = page
        .hits
        .iter()
        .chain(&next.hits)
        .map(|hit| &hit.ref_id)
        .collect();
    assert_eq!(ids.len(), 25);
    assert_eq!(f.core.image()?.as_bytes(), image.as_bytes());
    for args in [
        json!({"query":"Paraty","space":"other"}),
        json!({"query":"Paraty","owner":"other"}),
        json!({"query":"Paraty","limit":21}),
        json!({"query":"Paraty","cursor":cursor}),
    ] {
        assert!(DeviceCore::restore(
            NativeImage::decode_bounded(image.as_bytes().to_vec())?,
            f.credential()
        )?
        .search_reply_history(&intent, &args, 400)
        .is_err());
    }
    f.message("Newer content changes the actual source frontier")?;
    assert!(DeviceCore::restore(f.core.image()?, f.credential())?
        .search_reply_history(
            &intent,
            &json!({"query":"Paraty itinerary","cursor":cursor}),
            400
        )
        .is_err());
    Ok(())
}

#[test]
fn native_history_search_rechecks_expiry_replacement_and_revocation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for change in ["expiry", "replace", "revoke"] {
        let mut f = Fixture::new()?;
        f.grant("read-original", Some(900))?;
        let source = f.message("Retained history needle")?;
        let original = f.core.reply(SPACE, &source, 400)?.intent().clone();
        match change {
            "replace" => {
                f.grant("new-read-scope", None)?;
            }
            "revoke" => {
                f.encrypted(
                    EventBody::GrantRevoked {
                        grant: "read-original".into(),
                    },
                    450,
                )?;
            }
            _ => {}
        }
        let now = if change == "expiry" { 900 } else { 500 };
        assert!(DeviceCore::restore(f.core.image()?, f.credential())?
            .search_reply_history(&original, &json!({"query":"needle"}), now)
            .is_err());
    }
    Ok(())
}

#[test]
fn native_history_search_turns_replay_current_pages_without_relaxing_cursors(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    f.grant("turn-history", None)?;
    for number in 0..7 {
        f.message(&format!("needle memory {number}"))?;
    }
    let source = f.message("Find older details")?;
    let intent = f.core.reply(SPACE, &source, 400)?.intent().clone();
    let arguments = json!({"query":"needle","limit":2});
    let first = DeviceCore::restore(f.core.image()?, f.credential())?.continue_reply_history(
        &intent,
        &[],
        &arguments,
        400,
    )?;
    let cursor = first.next_cursor.clone().ok_or("cursor")?;
    let mut reads = vec![ReplyHistoryRead {
        arguments,
        page: first,
    }];
    f.message("An unrelated new message while the model thinks")?;
    let image = f.core.image()?;
    let arguments = json!({"query":"needle","limit":2,"cursor":cursor});
    // The public paging contract still refuses stale source-bound cursors.
    assert!(DeviceCore::restore(f.core.image()?, f.credential())?
        .search_reply_history(&intent, &arguments, 400)
        .is_err());
    let second = DeviceCore::restore(f.core.image()?, f.credential())?
        .continue_reply_history(&intent, &reads, &arguments, 400)?;
    assert_eq!(second.hits.len(), 2);
    assert!(reads[0]
        .page
        .hits
        .iter()
        .all(|hit| second.hits.iter().all(|other| other.ref_id != hit.ref_id)));
    reads.push(ReplyHistoryRead {
        arguments,
        page: second,
    });
    DeviceCore::restore(f.core.image()?, f.credential())?
        .verify_reply_history(&intent, &reads, 400)?;
    assert_eq!(f.core.image()?.as_bytes(), image.as_bytes());
    for arguments in [
        json!({"query":"needle","cursor":"invented"}),
        json!({"query":"different","cursor":cursor}),
    ] {
        assert!(DeviceCore::restore(f.core.image()?, f.credential())?
            .continue_reply_history(&intent, &reads, &arguments, 400)
            .is_err());
    }
    f.encrypted(
        EventBody::MessageAnnotated {
            target: reads[0].page.hits[0].ref_id.clone(),
            note: types::Annotation::Delete,
        },
        450,
    )?;
    assert!(DeviceCore::restore(f.core.image()?, f.credential())?
        .verify_reply_history(&intent, &reads, 400)
        .is_err());
    Ok(())
}

#[test]
fn native_history_search_turns_reject_replaced_permission_and_forged_pages(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    f.grant("turn-original", None)?;
    let source = f.message("needle detail")?;
    let intent = f.core.reply(SPACE, &source, 400)?.intent().clone();
    let arguments = json!({"query":"needle"});
    let page = DeviceCore::restore(f.core.image()?, f.credential())?.continue_reply_history(
        &intent,
        &[],
        &arguments,
        400,
    )?;
    let mut reads = vec![ReplyHistoryRead { arguments, page }];
    let original_ref = reads[0].page.hits[0].ref_id.clone();
    reads[0].page.hits[0].ref_id = "forged-message".into();
    assert!(DeviceCore::restore(f.core.image()?, f.credential())?
        .verify_reply_history(&intent, &reads, 400)
        .is_err());
    reads[0].page.hits[0].ref_id = original_ref;
    DeviceCore::restore(f.core.image()?, f.credential())?
        .verify_reply_history(&intent, &reads, 400)?;
    f.grant("turn-replacement", None)?;
    assert!(DeviceCore::restore(f.core.image()?, f.credential())?
        .verify_reply_history(&intent, &reads, 400)
        .is_err());
    Ok(())
}

#[test]
fn pending_reply_does_not_invalidate_its_own_retained_history_read(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    f.grant("history-self-reply", None)?;
    let source = f.message("needle detail")?;
    let intent = f.core.reply(SPACE, &source, 400)?.intent().clone();
    let arguments = json!({"query":"needle","limit":1});
    let page = DeviceCore::restore(f.core.image()?, f.credential())?
        .search_reply_history(&intent, &arguments, 400)?;
    let reads = vec![ReplyHistoryRead { arguments, page }];
    let mut core = DeviceCore::restore(f.core.image()?, f.credential())?;
    let prepared = core.prepare_reply(&intent, "needle result", 401)?;
    DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec())?,
        f.credential(),
    )?
    .verify_reply_history(&intent, &reads, 402)?;
    Ok(())
}
