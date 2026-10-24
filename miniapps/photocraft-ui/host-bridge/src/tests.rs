use super::*;
use photocraft_doc::{Layer, Rect, Size};
use photocraft_format::zip::{ZipReader, ZipWriter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn seed() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = Document::new(
        "Owned image",
        Size::new(2, 1),
        ColorMode::Rgb,
        SampleType::U8,
    );
    let mut layer = Layer::raster("Pixels", doc.pixel_format());
    let surface = layer.surface_mut().ok_or("missing real raster surface")?;
    surface.fill_rect(Rect::new(0, 0, 1, 1), &[1.0, 0.0, 0.0, 1.0]);
    surface.fill_rect(Rect::new(1, 0, 1, 1), &[0.0, 0.0, 1.0, 1.0]);
    doc.layers.push(layer);
    Ok(photocraft_format::save_to_bytes(
        &doc,
        &SaveOptions::default(),
    )?)
}
fn sha(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn changed_manifest(
    bytes: &[u8],
    change: impl FnOnce(&mut serde_json::Value),
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let archive = ZipReader::new(bytes)?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&archive.read_by_name("manifest.json", MAX_MANIFEST_BYTES)?)?;
    change(&mut manifest);
    let mut writer = ZipWriter::new();
    for entry in &archive.entries {
        let value = if entry.name == "manifest.json" {
            serde_json::to_vec(&manifest)?
        } else {
            archive.read_by_name(&entry.name, MAX_DOCUMENT_BYTES)?
        };
        writer.add(&entry.name, &value)?;
    }
    Ok(writer.finish()?)
}

#[test]
fn upstream_flip_then_cold_png_preserves_real_pixels_and_source() -> TestResult {
    let source = seed()?;
    let retained = source.clone();
    let first = inspect(&source, &sha(&source))?;
    assert_eq!((first.width, first.height), (2, 1));
    let changed = transform(&source, &sha(&source), Operation::FlipHorizontal)?;
    assert_eq!(
        changed,
        transform(&source, &sha(&source), Operation::FlipHorizontal)?
    );
    // Export opens the saved bytes in a new actual engine/document path.
    let png = export_png(&changed, &sha(&changed))?;
    assert_eq!(
        photocraft_codecs::decode(&png)?.to_rgba8(),
        [0, 0, 255, 255, 255, 0, 0, 255]
    );
    let restored = transform(&changed, &sha(&changed), Operation::FlipHorizontal)?;
    assert_eq!(
        archive::check(&source)?.document,
        archive::check(&restored)?.document
    );
    assert_eq!(source, retained);
    Ok(())
}

#[test]
fn four_actual_right_angle_commands_reopen_same_native_document() -> TestResult {
    let source = seed()?;
    let mut current = source.clone();
    for _ in 0..4 {
        current = transform(&current, &sha(&current), Operation::RotateClockwise)?;
        inspect(&current, &sha(&current))?;
    }
    assert_eq!(
        archive::check(&source)?.document,
        archive::check(&current)?.document
    );
    Ok(())
}

#[test]
fn unknown_future_and_nested_data_refuse_without_lossy_rewrite() -> TestResult {
    let source = seed()?;
    let changes: [fn(&mut serde_json::Value); 3] = [
        |v: &mut serde_json::Value| v["future"] = serde_json::json!(true),
        |v: &mut serde_json::Value| v["document"]["future"] = serde_json::json!({"retained":7}),
        |v: &mut serde_json::Value| v["format_version"] = serde_json::json!(2),
    ];
    for change in changes {
        let bytes = changed_manifest(&source, change)?;
        assert_eq!(
            transform(&bytes, &sha(&bytes), Operation::FlipVertical),
            Err(Error::UnsupportedProfile)
        );
    }
    Ok(())
}

#[test]
fn stale_digest_limits_and_external_commands_fail_before_engine_effect() -> TestResult {
    let source = seed()?;
    assert_eq!(inspect(&source, &[0; 32]), Err(Error::SourceMismatch));
    let huge = vec![0; MAX_DOCUMENT_BYTES + 1];
    assert_eq!(inspect(&huge, &[0; 32]), Err(Error::Limit));
    for command in [
        "file.open",
        "file.save",
        "run_command",
        "batch",
        "plugin.run",
    ] {
        assert!(authorize(command, &serde_json::json!({})).is_err());
        assert!(serde_json::from_value::<Operation>(serde_json::json!(command)).is_err());
    }
    assert!(
        authorize(
            Operation::FlipHorizontal.command(),
            &serde_json::json!({"path":"outside"})
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn archive_duplicates_and_unreferenced_entries_refuse() -> TestResult {
    let source = seed()?;
    let original = ZipReader::new(&source)?;
    for duplicate in [true, false] {
        let mut writer = ZipWriter::new();
        for entry in &original.entries {
            writer.add(
                &entry.name,
                &original.read_by_name(&entry.name, MAX_DOCUMENT_BYTES)?,
            )?;
        }
        if duplicate {
            writer.add(
                "manifest.json",
                &original.read_by_name("manifest.json", MAX_MANIFEST_BYTES)?,
            )?;
        } else {
            writer.add("unclaimed.txt", b"must not silently disappear")?;
        }
        let bad = writer.finish()?;
        assert_eq!(inspect(&bad, &sha(&bad)), Err(Error::UnsupportedProfile));
    }
    Ok(())
}

#[test]
fn bounded_archive_and_canvas_fail_before_decode_or_command() -> TestResult {
    let source = seed()?;
    let oversized = changed_manifest(&source, |m| {
        m["document"]["size"]["width"] = serde_json::json!(1025);
    })?;
    assert_eq!(inspect(&oversized, &sha(&oversized)), Err(Error::Limit));
    let mut writer = ZipWriter::new();
    writer.add("manifest.json", &vec![b' '; MAX_MANIFEST_BYTES + 1])?;
    let large = writer.finish()?;
    assert_eq!(inspect(&large, &sha(&large)), Err(Error::Limit));
    let mut writer = ZipWriter::new();
    for n in 0..129 {
        writer.add(&format!("blobs/{n:064x}.zst"), b"")?;
    }
    let excessive = writer.finish()?;
    assert_eq!(inspect(&excessive, &sha(&excessive)), Err(Error::Limit));
    let outside = changed_manifest(&source, |m| {
        m["document"]["layers"][0]["content"]["surface"]["tiles"][0]["tx"] =
            serde_json::json!(i32::MAX);
    })?;
    assert_eq!(
        inspect(&outside, &sha(&outside)),
        Err(Error::UnsupportedProfile)
    );
    Ok(())
}

#[test]
fn sparse_raster_crosses_real_tile_boundaries_without_losing_original_pixels() -> TestResult {
    let source = changed_manifest(&seed()?, |m| {
        m["document"]["size"]["width"] = serde_json::json!(513);
    })?;
    let flipped = transform(&source, &sha(&source), Operation::FlipHorizontal)?;
    let png = export_png(&flipped, &sha(&flipped))?;
    let pixels = photocraft_codecs::decode(&png)?.to_rgba8();
    assert_eq!(pixels.len(), 513 * 4);
    assert!(pixels[..511 * 4].iter().all(|b| *b == 0));
    assert_eq!(&pixels[511 * 4..], [0, 0, 255, 255, 255, 0, 0, 255]);
    let restored = transform(&flipped, &sha(&flipped), Operation::FlipHorizontal)?;
    assert_eq!(
        archive::check(&source)?.document,
        archive::check(&restored)?.document
    );
    Ok(())
}
