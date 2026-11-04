use super::*;
#[test]
fn original_durability_cuts_never_advance_authority() -> TestResult {
    for point in [
        faults::Point::Write,
        faults::Point::FileSync,
        faults::Point::DirectorySync,
    ] {
        let home = Home::new()?;
        let mut writer = AnchorWriter::create(&home.0, deployment())?;
        let checkpoint = writer.reader().checkpoint()?;
        assert!(writer.reader().retained_originals()?.is_empty());
        faults::fail(point);
        assert_eq!(
            writer.retain_original(b"opaque encrypted incurred result"),
            Err(AnchorError::Unavailable)
        );
        let key = writer.retain_original(b"opaque encrypted incurred result")?;
        assert_eq!(
            writer.reader().retained_originals()?.as_slice(),
            std::slice::from_ref(&key)
        );
        assert_eq!(
            writer.reader().retained_original(&key)?,
            b"opaque encrypted incurred result"
        );
        assert_eq!(
            writer.retain_original(b"opaque encrypted incurred result")?,
            key
        );
        assert_eq!(writer.reader().checkpoint()?, checkpoint);
        drop(writer);
        let writer = AnchorWriter::open(&home.0, deployment())?;
        assert_eq!(
            writer.reader().retained_original(&key)?,
            b"opaque encrypted incurred result"
        );
        assert_eq!(writer.reader().checkpoint()?, checkpoint);
        std::fs::write(
            home.0.join("originals").join(format!("{key}.bin")),
            b"changed ciphertext",
        )?;
        assert_eq!(
            writer.reader().retained_original(&key),
            Err(AnchorError::Unreconciled)
        );
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn original_storage_refuses_links_wrong_names_and_oversized_sources() -> TestResult {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let home = Home::new()?;
    let mut writer = AnchorWriter::create(&home.0, deployment())?;
    assert!(writer.retain_original(&vec![7; 512 * 1024 + 1]).is_err());
    let key = writer.retain_original(b"private original")?;
    let path = home.0.join("originals").join(format!("{key}.bin"));
    assert_eq!(
        std::fs::metadata(&path)?.permissions().mode() & 0o777,
        0o600
    );
    let original = home.0.join("opaque-source");
    std::fs::rename(&path, &original)?;
    symlink(&original, &path)?;
    assert_eq!(
        writer.reader().retained_original(&key),
        Err(AnchorError::Invalid)
    );
    assert!(writer.reader().retained_originals().is_err());
    std::fs::remove_file(&path)?;
    std::fs::rename(original, &path)?;
    assert!(writer.reader().retained_original("../../source").is_err());
    assert_eq!(
        writer.reader().retained_original(&key)?,
        b"private original"
    );
    Ok(())
}
