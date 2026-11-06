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
