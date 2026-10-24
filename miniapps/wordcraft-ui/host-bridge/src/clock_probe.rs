//! Qualification-only component: exercise both actual upstream clocks, without host dispatch.
use serde_json::json;
use std::io::{Cursor, Read};
use wordcraft_doc::Document;
use wordcraft_engine::Session;

fn main() -> Result<(), String> {
    let engine = wordcraft_engine::cmd::now_unix();
    let layout_before = wordcraft_layout::now_ms();
    std::thread::sleep(std::time::Duration::from_millis(25));
    let layout_after = wordcraft_layout::now_ms();
    let mut session = Session::new(Document::new());
    session.doc.settings.track_changes = true;
    session
        .run("text.insert", &json!({"text":"clock","raw":true}))
        .map_err(|_| "engine probe failed")?;
    // ODT requires a date on a tracked region. Blank only this synthetic revision's
    // date to exercise formats::odt's genuine now_unix fallback, not the engine date.
    session
        .doc
        .revisions
        .first_mut()
        .ok_or("missing actual revision")?
        .date
        .clear();
    let bytes = wordcraft_formats::odt::export(&session.doc)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "ODT probe failed")?;
    let file = zip
        .by_name("content.xml")
        .map_err(|_| "ODT content missing")?;
    if file.size() > 1024 * 1024 {
        return Err("ODT content too large".into());
    }
    let mut xml = String::new();
    file.take(1024 * 1024 + 1)
        .read_to_string(&mut xml)
        .map_err(|_| "ODT content invalid")?;
    if xml.len() > 1024 * 1024 {
        return Err("ODT content too large".into());
    }
    let (_, rest) = xml
        .split_once("<dc:date>")
        .ok_or("actual format clock absent")?;
    let (date, _) = rest
        .split_once("</dc:date>")
        .ok_or("actual format clock invalid")?;
    if date.len() > 32 {
        return Err("actual format date unbounded".into());
    }
    let after = wordcraft_engine::cmd::now_unix();
    println!(
        "{}",
        json!({"engineUnixSeconds":engine,"engineUnixAfter":after,
        "layoutBeforeMs":layout_before,"layoutAfterMs":layout_after,
        "engineIsoBefore":wordcraft_engine::cmd::iso_from_unix_secs(engine),
        "engineIsoAfter":wordcraft_engine::cmd::iso_from_unix_secs(after),"formatChangeDate":date})
    );
    Ok(())
}
