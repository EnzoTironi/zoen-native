use super::*;
use std::io::Write;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

fn with_padding(sizes: &[usize]) -> Vec<u8> {
    let mut seed = ZipArchive::new(Cursor::new(registration::EMPTY)).unwrap();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for index in 0..seed.len() {
        let mut entry = seed.by_index(index).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        writer.start_file(entry.name(), options).unwrap();
        writer.write_all(&bytes).unwrap();
    }
    for (index, size) in sizes.iter().enumerate() {
        writer
            .start_file(format!("opaque-{index}.bin"), options)
            .unwrap();
        writer.write_all(&vec![0; *size]).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

// Change only the local/central uncompressed-length fields, leaving the actual
// Deflate stream, compressed length and valid CRC unchanged.
fn declared_size(bytes: &mut [u8], name: &str, declared: u32) {
    let (local, central) = {
        let mut zip = ZipArchive::new(Cursor::new(&*bytes)).unwrap();
        let entry = zip.by_name(name).unwrap();
        assert_eq!(entry.compression(), CompressionMethod::Deflated);
        (
            entry.header_start() as usize,
            entry.central_header_start() as usize,
        )
    };
    assert_eq!(&bytes[local..local + 4], b"PK\x03\x04");
    assert_eq!(&bytes[central..central + 4], b"PK\x01\x02");
    bytes[local + 22..local + 26].copy_from_slice(&declared.to_le_bytes());
    bytes[central + 24..central + 28].copy_from_slice(&declared.to_le_bytes());
}

#[test]
fn genuine_seed_and_valid_deflate_members_keep_exact_bytes_and_slide_metadata() {
    let original = registration::EMPTY.to_vec();
    assert_eq!(Document::parse(&original).unwrap().slides, 1);
    assert_eq!(original, registration::EMPTY);
    let padded = with_padding(&[5 << 20, 5 << 20]);
    assert!(padded.len() < MAX_BYTES);
    assert_eq!(Document::parse(&padded).unwrap().slides, 1);
}

#[test]
fn forged_deflate_uncompressed_lengths_are_rejected_despite_valid_crc() {
    let original = with_padding(&[4096]);
    for declared in [0, 1, 4095, 4097] {
        let mut forged = original.clone();
        declared_size(&mut forged, "opaque-0.bin", declared);
        // The pinned ZIP reader verifies CRC, but does not enforce this length.
        let mut archive = ZipArchive::new(Cursor::new(&forged)).unwrap();
        let mut entry = archive.by_name("opaque-0.bin").unwrap();
        assert_eq!(entry.size(), u64::from(declared));
        let mut decoded = Vec::new();
        entry.read_to_end(&mut decoded).unwrap();
        assert_eq!(decoded, vec![0; 4096]);
        assert_eq!(
            Document::parse(&forged).err(),
            Some("Invalid DeckCraft entry size")
        );
    }
}

#[test]
fn valid_declared_members_above_total_inflation_budget_are_rejected() {
    let padded = with_padding(&[6 << 20, 6 << 20, 6 << 20]);
    assert!(padded.len() < MAX_BYTES);
    let archive = ZipArchive::new(Cursor::new(&padded)).unwrap();
    assert!(archive.decompressed_size().unwrap() > u128::from(MAX_INFLATED_BYTES));
    assert_eq!(
        Document::parse(&padded).err(),
        Some("Oversized DeckCraft container")
    );
}

#[test]
fn forged_small_headers_cannot_hide_an_over_total_deflate_payload() {
    let mut forged = with_padding(&[6 << 20, 6 << 20, 6 << 20]);
    for index in 0..3 {
        declared_size(&mut forged, &format!("opaque-{index}.bin"), 1);
    }
    let mut archive = ZipArchive::new(Cursor::new(&forged)).unwrap();
    assert!(archive.decompressed_size().unwrap() < u128::from(MAX_INFLATED_BYTES));
    let mut actual = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        actual += bytes.len() as u64;
    }
    assert!(actual > MAX_INFLATED_BYTES);
    assert_eq!(
        Document::parse(&forged).err(),
        Some("Invalid DeckCraft entry size")
    );
}
