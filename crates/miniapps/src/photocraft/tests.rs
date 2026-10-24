use super::*;
use std::io::Write;

type R<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn rewrite(raw: &str, extra: Option<(&str, &[u8])>) -> R<Vec<u8>> {
    let mut source = zip::ZipArchive::new(Cursor::new(EMPTY))?;
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for i in 0..source.len() {
        let mut entry = source.by_index(i)?;
        output.start_file(entry.name(), options)?;
        if entry.name() == "manifest.json" {
            output.write_all(raw.as_bytes())?;
        } else {
            std::io::copy(&mut entry, &mut output)?;
        }
    }
    if let Some((name, bytes)) = extra {
        output.start_file(name, options)?;
        output.write_all(bytes)?;
    }
    Ok(output.finish()?.into_inner())
}
#[test]
fn native_seed_is_structurally_accepted_without_rewriting_source() -> R {
    let original = EMPTY.to_vec();
    let doc = Document::parse(EMPTY)?;
    assert_eq!((doc.width, doc.height, doc.layers), (640, 480, 1));
    let meta: Value = serde_json::from_str(&doc.metadata("Image"))?;
    assert_eq!(meta["engineRevision"], ENGINE_REVISION);
    assert_eq!(EMPTY, original);
    Ok(())
}
#[test]
fn unknown_nested_fields_profiles_tile_coordinates_and_references_are_refused() -> R {
    let (manifest, _) = container(EMPTY)?;
    for (path, value) in [
        (
            "/document/size",
            json!({"width":640,"height":480,"hidden":true}),
        ),
        ("/document/size/width", json!(0)),
        ("/document/size/height", json!(1025)),
        ("/document/name", json!("bad\nname")),
        ("/document/mode", json!("Cmyk")),
        ("/document/layers/0/opacity", json!(0.5)),
        ("/document/layers/0/content/surface/tiles/0/tx", json!(9)),
        (
            "/document/layers/0/content/surface/tiles/0/hash",
            json!("0".repeat(64)),
        ),
        (
            "/document/layers/0/content/surface/default",
            json!("0000000G"),
        ),
        ("/document/metadata/exif", json!({"private":"unsupported"})),
    ] {
        let mut bad = manifest.clone();
        *bad.pointer_mut(path).ok_or("test field missing")? = value;
        assert!(
            Document::parse(&rewrite(&bad.to_string(), None)?).is_err(),
            "{path}"
        );
    }
    let raw = manifest.to_string();
    for (name, data) in [
        ("../escape", b"x".as_slice()),
        ("tiles/extra.zst", b"x"),
        ("secret", b"x"),
    ] {
        assert!(Document::parse(&rewrite(&raw, Some((name, data)))?).is_err());
    }
    let mut duplicate = manifest;
    let tiles = duplicate
        .pointer_mut("/document/layers/0/content/surface/tiles")
        .and_then(Value::as_array_mut)
        .ok_or("test tiles missing")?;
    tiles.push(tiles[0].clone());
    assert!(Document::parse(&rewrite(&duplicate.to_string(), None)?).is_err());
    Ok(())
}
#[test]
fn duplicate_json_fields_and_unbounded_containers_are_refused() -> R {
    let (value, _) = container(EMPTY)?;
    let raw = value.to_string();
    for bad in [
        raw.replacen(
            "\"format_version\":1",
            "\"format_version\":1,\"format_version\":1",
            1,
        ),
        raw.replacen("\"width\":640", "\"width\":640,\"width\":640", 1),
        format!("{raw} {{}}"),
    ] {
        assert_ne!(raw, bad);
        assert!(Document::parse(&rewrite(&bad, None)?).is_err());
    }
    assert!(Document::parse(&[]).is_err());
    assert!(Document::parse(&vec![0; MAX_BYTES + 1]).is_err());
    assert!(Document::parse(&EMPTY[..EMPTY.len() - 1]).is_err());
    Ok(())
}
