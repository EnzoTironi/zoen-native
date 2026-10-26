//! The ordinary Client receives the parsed MIME; real signed relay transport
//! and a cold reopen preserve the File without installing or granting an App.
use super::*;
use crate::file_import::Input;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_import_preserves_explicit_mime_defaults_and_bytes_across_sync_and_cold_open(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url)?;
    online(&f.client, true).await;
    f.client.stop_sync();
    let requests_before = serde_json::to_value(f.client.requests())?;
    let mut imported = Vec::new();
    for (name, mime, expected) in [
        (
            "package.html",
            Some("text/html;profile=mcp-app"),
            "text/html;profile=mcp-app",
        ),
        ("ordinary.HTML", None, "application/octet-stream"),
        ("notes.md", None, "text/markdown"),
    ] {
        let mut args = vec![f.space.clone(), name.into()];
        if let Some(mime) = mime {
            args.extend(["--mime".into(), mime.into()]);
        }
        let input = Input::parse(&args)?;
        let bytes = format!("<!doctype html><title>{name}</title><p>Synthetic private file</p>")
            .into_bytes();
        let item = input.add(&f.client, f.space.clone(), bytes.clone(), None)?;
        assert_eq!(item.kind_id, "file");
        assert_eq!(item.file.as_ref().unwrap().mime, expected);
        assert!(item.app.is_none());
        imported.push((item.id, expected, bytes));
    }
    // Use the fixture's genuine signed/verified publication path. Do not mark
    // uploads done: the normal sync worker must receive a real signed PUT ack.
    f.confirm_publications()?;
    online(&f.client, true).await;
    assert!(!server.uploads.lock().unwrap().is_empty());
    f.reopen();
    for (id, mime, bytes) in imported {
        let file = f.client.file_info(id.clone())?.unwrap();
        assert_eq!(file.mime, mime);
        assert_eq!(f.client.file_bytes(id, None)?, Some(bytes));
    }
    assert_eq!(serde_json::to_value(f.client.requests())?, requests_before);
    assert!(server.cursors.lock().unwrap().is_empty());
    assert!(!server.calls.lock().unwrap().contains(&"read_receipt"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_mime_does_not_grant_foreign_conversation_membership(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url)?;
    let before = serde_json::to_value(f.client.items())?;
    let input = Input::parse(&[
        "foreign".into(),
        "app.html".into(),
        "--mime".into(),
        "text/html;profile=mcp-app".into(),
    ])?;
    assert!(matches!(
        input.add(
            &f.client,
            "foreign".into(),
            b"<!doctype html><p>Inert</p>".to_vec(),
            None
        ),
        Err(client_core::CoreError::Forbidden { .. })
    ));
    assert_eq!(serde_json::to_value(f.client.items())?, before);
    assert!(server.uploads.lock().unwrap().is_empty());
    assert!(server.calls.lock().unwrap().is_empty());
    Ok(())
}
