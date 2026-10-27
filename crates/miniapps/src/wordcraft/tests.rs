use super::*;
use std::io::Write;
const CHANGED: &[u8] = include_bytes!("fixtures/actual-wasi.docx");
const SPECIAL: &[u8] = include_bytes!("fixtures/actual-special.docx");
fn rewrite(original: &[u8], name: &str, content: &[u8]) -> Vec<u8> {
    let original = parts(original).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (part, data) in &original {
        writer
            .start_file(
                part,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer
            .write_all(if part == name { content } else { data })
            .unwrap();
    }
    if !original.contains_key(name) {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(content).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
#[test]
fn real_writer_and_wasip2_exports_have_the_same_closed_profile_without_exposing_text_metadata() {
    assert_eq!(Document::parse(EMPTY).unwrap().text, "");
    let doc = Document::parse(CHANGED).unwrap();
    assert_eq!(doc.text, "WASI real\nOlá 日本語 👩🏽‍💻\ttexto\n");
    assert_eq!((doc.paragraphs, doc.words), (3, 5));
    let special = Document::parse(SPECIAL).unwrap();
    assert_eq!(special.text, "  <&>\"'\t\t\n\nOlá 日本語 👩🏽‍💻\t fim\n");
    let metadata: serde_json::Value = serde_json::from_str(&doc.metadata("Title")).unwrap();
    assert_eq!(metadata["engineRevision"], ENGINE_REVISION);
    assert!(!metadata.to_string().contains("WASI real"));
    assert_eq!(
        crate::binary::Format::for_id(APP),
        Some(crate::binary::Format::Wordcraft)
    );
    assert_eq!(
        crate::spec(APP).is_some(),
        crate::binary::Format::Wordcraft.available()
    );
    assert_eq!(
        crate::binary::agent_tools::tools_for(crate::binary::Format::Wordcraft).len(),
        4
    );
}
#[test]
fn opaque_parts_macro_metadata_relationship_and_body_changes_are_refused_without_normalization() {
    let known = parts(CHANGED).unwrap();
    for (name, content) in [
        ("word/vbaProject.bin", b"macro".as_slice()),
        ("customXml/item1.xml", b"<private/>".as_slice()),
        ("../escape", b"outside".as_slice()),
        ("word/styles.xml", b"<styles/>".as_slice()),
        (
            "word/_rels/document.xml.rels",
            b"<external Target=\"https://example.invalid\"/>".as_slice(),
        ),
        (
            "docProps/core.xml",
            b"<author>unsupported</author>".as_slice(),
        ),
    ] {
        assert!(
            Document::parse(&rewrite(CHANGED, name, content)).is_err(),
            "{name}"
        );
    }
    let xml = std::str::from_utf8(&known["word/document.xml"]).unwrap();
    for altered in [
        xml.replace("WASI real", "<w:hyperlink>WASI real</w:hyperlink>"),
        xml.replace("WASI real", "WASI &#114;eal"),
        xml.replace("WASI real", "WASI\u{fffe}real"),
        xml.replace("<w:p>", "<w:p private=\"ignored\">"),
        xml.replace("<w:body>", "<!DOCTYPE x><w:body>"),
    ] {
        assert!(
            Document::parse(&rewrite(CHANGED, "word/document.xml", altered.as_bytes())).is_err()
        );
    }
    let app = std::str::from_utf8(&known["docProps/app.xml"])
        .unwrap()
        .replace("<Words>5</Words>", "<Words>4</Words>");
    assert!(Document::parse(&rewrite(CHANGED, "docProps/app.xml", app.as_bytes())).is_err());
}
#[test]
fn central_directory_duplicates_declared_bombs_counts_and_byte_limits_refuse_before_inflation() {
    let mut duplicate = rewrite(
        CHANGED,
        "word/forgedxx.xml",
        &parts(CHANGED).unwrap()["word/document.xml"],
    );
    let old = b"word/forgedxx.xml";
    let new = b"word/document.xml";
    let mut replaced = 0;
    for i in 0..=duplicate.len() - old.len() {
        if &duplicate[i..i + old.len()] == old {
            duplicate[i..i + new.len()].copy_from_slice(new);
            replaced += 1;
        }
    }
    assert_eq!(replaced, 2);
    assert!(Document::parse(&duplicate).is_err());
    let bomb = rewrite(CHANGED, "word/huge.xml", &vec![b' '; MAX_PART + 1]);
    assert!(bomb.len() < MAX_BYTES);
    assert!(Document::parse(&bomb).is_err());
    let mut declared = CHANGED.to_vec();
    let central = declared
        .windows(4)
        .position(|w| w == b"PK\x01\x02")
        .unwrap();
    declared[central + 24..central + 28].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Document::parse(&declared).is_err());
    let mut count = CHANGED.to_vec();
    let end = count.len() - 22;
    count[end + 8..end + 10].copy_from_slice(&33u16.to_le_bytes());
    count[end + 10..end + 12].copy_from_slice(&33u16.to_le_bytes());
    assert!(Document::parse(&count).is_err());
    assert!(Document::parse(&vec![0; MAX_BYTES + 1]).is_err());
    assert!(Document::parse(&CHANGED[..CHANGED.len() - 1]).is_err());
    for text in [
        "bad\0text",
        "bad\rtext",
        "bad\u{fffe}text",
        "bad\u{ffff}text",
    ] {
        assert!(!valid_text(text));
    }
}

#[test]
fn genuine_rich_word_model_is_accepted_without_exposing_its_content_in_metadata(
) -> Result<(), &'static str> {
    use std::sync::Arc;
    use zoen_word_profile::model::{props::Align, Block, Document as Model};
    let mut model = Model::from_text("Styled document");
    let Block::Para(paragraph) = Arc::make_mut(&mut model.body[0]) else {
        return Err("missing actual paragraph");
    };
    paragraph.props.align = Some(Align::Center);
    paragraph.runs[0].props.bold = Some(true);
    let bytes = zoen_word_profile::encode(&model)?;
    let parsed = Document::parse(&bytes)?;
    assert_eq!(parsed.text, "Styled document");
    assert_eq!((parsed.paragraphs, parsed.words), (1, 2));
    assert!(!parsed.metadata("Title").contains("Styled document"));
    assert_eq!(zoen_word_profile::parse(&bytes)?, model);
    Ok(())
}
