//! Actual upstream PDF bytes through the shared encrypted Item/CAS authority.
#![warn(clippy::pedantic, clippy::perf)]
use super::*;
use miniapps::pdfcraft;
const PDF: &[u8] = include_bytes!("../../../../miniapps/src/pdfcraft/fixtures/actual-source.pdf");
const ROTATED: &[u8] =
    include_bytes!("../../../../miniapps/src/pdfcraft/fixtures/actual-wasi-rotated.pdf");
fn pdf(f: &Fixture, mutation: &str) -> R<AppBinaryDocumentDto> {
    f.client.app_binary_create_for(
        pdfcraft::APP.into(),
        f.space.clone(),
        "Reviewed PDF".into(),
        STANDARD.encode(PDF),
        mutation.into(),
    )
}

#[test]
fn pdf_creation_owns_encrypted_exact_bytes_and_format_scoped_identity(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let doc = pdf(&f, "shared-intent")?;
    let before = counts(&f);
    assert_eq!(pdf(&f, "shared-intent")?, doc);
    assert_eq!(counts(&f), before);
    assert!(doc.item_id.starts_with("it_pdf_"));
    assert_eq!(doc.kind, pdfcraft::APP);
    assert_eq!(doc.engine_revision, pdfcraft::ENGINE_REVISION);
    assert_eq!(doc.actor_kind, "human");
    assert_eq!(STANDARD.decode(&doc.data_base64)?, PDF);
    let e = f.client.lock();
    let item = e.item_state(&doc.item_id)?;
    let ItemContent::App(app) = &item.current().content else {
        return Err("expected canonical PDF App".into());
    };
    assert_eq!(app.resource_uri, pdfcraft::RESOURCE);
    let file = app.document.as_ref().ok_or("missing canonical PDF file")?;
    assert_eq!(file.mime, "application/pdf");
    assert_eq!(file.name, "document.pdf");
    assert_eq!(
        file.app_origin
            .as_ref()
            .ok_or("missing source origin")?
            .author,
        doc.actor_id
    );
    assert_ne!(file.chunks, Vec::<types::ChunkRef>::new());
    assert!(file
        .chunks
        .iter()
        .all(|c| c.blob != c.sha256 && !c.key.is_empty()));
    assert!(!app.state_json.contains("data_base64"));
    assert_eq!(
        e.file_bytes(&doc.item_id, Some(1))?
            .ok_or("missing bytes")?,
        PDF
    );
    Ok(())
}

#[test]
fn pdf_cas_lost_ack_cold_history_and_undo_save_keep_chosen_bytes(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(types::new_id("pdf-cas")?);
    std::fs::create_dir(&dir)?;
    let path = dir.join("pdf.sqlite");
    let f = fixture(path.to_str().ok_or("invalid fixture path")?);
    let base = pdf(&f, "create")?;
    let accepted = save(&f, &base, ROTATED, "lost-ack")?;
    let undo = save(&f, &accepted, PDF, "undo-new-save")?;
    assert_eq!(undo.version, 3);
    assert_eq!(undo.sha256, base.sha256);
    let before = counts(&f);
    assert!(save(&f, &base, ROTATED, "stale-write").is_err());
    assert_eq!(counts(&f), before);
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path.to_string_lossy().into(), "en".into())?;
    assert!(client.unlock(vault)?);
    let retry = client.app_binary_save(
        base.item_id.clone(),
        base.version,
        base.sha256.clone(),
        STANDARD.encode(ROTATED),
        "lost-ack".into(),
        hash(b"test operation"),
    )?;
    assert_eq!(retry.version, 2);
    assert_eq!(retry.current_version, 3);
    assert_eq!(retry.sha256, accepted.sha256);
    assert_eq!(STANDARD.decode(&retry.data_base64)?, ROTATED);
    assert_eq!(
        client.app_binary_read(base.item_id.clone(), None, None)?,
        undo
    );
    assert_eq!(
        client
            .app_binary_read(base.item_id.clone(), Some(1), Some(base.sha256))?
            .data_base64,
        STANDARD.encode(PDF)
    );
    assert_eq!(client.item(base.item_id)?.versions.len(), 3);
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn pdf_wrong_format_actor_and_scope_fail_without_partial_writes() -> R<()> {
    let f = fixture(":memory:");
    let doc = pdf(&f, "create")?;
    let deck = create(&f, "create");
    let before = counts(&f);
    assert!(save(&f, &doc, ORIGINAL, "wrong-format").is_err());
    assert!(save(&f, &deck, PDF, "wrong-format").is_err());
    assert!(f
        .client
        .app_binary_create_for(
            "pdfcraft".into(),
            "foreign-space".into(),
            "No authority".into(),
            STANDARD.encode(PDF),
            "foreign".into()
        )
        .is_err());
    assert!(f
        .client
        .app_binary_save(
            "foreign-item".into(),
            doc.version,
            doc.sha256.clone(),
            STANDARD.encode(ROTATED),
            "foreign".into(),
            hash(b"test operation")
        )
        .is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(
        f.client.app_binary_read(doc.item_id.clone(), None, None)?,
        doc
    );
    Ok(())
}
