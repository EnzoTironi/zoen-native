use super::*;
use std::io::Write;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

// Standards-shaped adversarial fixtures, not a substitute GridCraft engine.
// Real Session-generated seed/reopen/export qualification is separate.
fn parts(cells: &str) -> Vec<(String, String)> {
    vec![
        ("[Content_Types].xml".into(), String::from(r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#)),
        ("_rels/.rels".into(), r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="root" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.into()),
        ("xl/workbook.xml".into(), r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="sheet1"/></sheets></workbook>"#.into()),
        ("xl/_rels/workbook.xml.rels".into(), r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="sheet1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.into()),
        ("xl/worksheets/sheet1.xml".into(), format!(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1">{cells}</row></sheetData></worksheet>"#)),
    ]
}
fn archive(parts: Vec<(String, String)>) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in parts {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
#[test]
fn profile_reads_bounded_cell_metadata_without_rewriting_any_input_byte() {
    let bytes = archive(parts(
        r#"<c r="A1"><v>4</v></c><c r="B1"><f>A1*2</f><v>8</v></c>"#,
    ));
    let retained = bytes.clone();
    let doc = Document::parse(&bytes).unwrap();
    assert_eq!((doc.sheets, doc.cells), (1, 2));
    assert_eq!(bytes, retained);
    let metadata: serde_json::Value = serde_json::from_str(&doc.metadata("Budget")).unwrap();
    assert_eq!(metadata["engineRevision"], ENGINE_REVISION);
    assert_eq!(metadata["title"], "Budget");
}
#[test]
fn profile_refuses_unsupported_zip_parts_traversal_macros_and_future_extensions() {
    for name in [
        "../escape",
        "xl/vbaProject.bin",
        "xl/unknown.xml",
        "xl/externalLinks/externalLink1.xml",
        "xl/worksheets/../sheet1.xml",
        "xl/worksheets/sheet01.xml",
        "/xl/workbook.xml",
        "xl\\workbook.xml",
    ] {
        let mut input = parts("");
        input.push((name.into(), "unsupported".into()));
        assert!(Document::parse(&archive(input)).is_err(), "{name}");
    }
}
#[test]
fn profile_refuses_missing_parts_unbound_duplicate_sheet_targets_and_wrong_xml_roots() {
    let original = parts("");
    for index in 0..original.len() {
        let mut input = original.clone();
        input.remove(index);
        assert!(Document::parse(&archive(input)).is_err(), "missing {index}");
    }
    for (index, from, to) in [
        (2, "r:id=\"sheet1\"", "r:id=\"missing\""),
        (
            2,
            "</sheets>",
            "<sheet name=\"Two\" sheetId=\"2\" r:id=\"sheet1\"/></sheets>",
        ),
        (3, "worksheets/sheet1.xml", "worksheets/sheet2.xml"),
        (4, "worksheet", "arbitrary"),
        (
            0,
            "spreadsheetml.sheet.main+xml",
            "ms-excel.sheet.macroEnabled.main+xml",
        ),
    ] {
        let mut input = original.clone();
        input[index].1 = input[index].1.replace(from, to);
        assert!(Document::parse(&archive(input)).is_err(), "{index}:{from}");
    }
}
#[test]
fn profile_cell_limits_are_not_silent_truncation_or_duplicate_repair() {
    assert!(cell_reference("IV10000"));
    for key in ["A0", "A01", "IW1", "A10001", "$A$1", "a1", "A1:B2", "É1"] {
        assert!(!cell_reference(key), "{key}");
        assert!(
            Document::parse(&archive(parts(&format!(r#"<c r="{key}"><v>4</v></c>"#)))).is_err()
        );
    }
    assert!(Document::parse(&archive(parts(r#"<c r="A1"/><c r="A1"/>"#))).is_err());
}
#[test]
fn profile_rejects_external_relationships_and_entity_encoded_external_formulas() {
    for formula in [
        "[other.xlsx]Sheet1!A1",
        "&#91;other.xlsx&#93;Sheet1!A1",
        "WEBSERVICE (\"https://example.invalid\")",
        "HYPERLINK(\"https://example.invalid\")",
        "server|topic!A1",
    ] {
        assert!(
            Document::parse(&archive(parts(&format!(
                r#"<c r="A1"><f>{formula}</f></c>"#
            ))))
            .is_err(),
            "{formula}"
        );
    }
    let mut input = parts("");
    input[3].1 = input[3]
        .1
        .replace("Target=", "TargetMode=\"External\" Target=");
    assert!(Document::parse(&archive(input)).is_err());
    let mut input = parts("");
    input[3].1 = input[3]
        .1
        .replace("worksheets/sheet1.xml", "../outside.xml");
    assert!(Document::parse(&archive(input)).is_err());
}
#[test]
fn profile_keeps_unicode_and_predefined_entities_but_never_accepts_dtds() {
    assert!(Document::parse(&archive(parts(r#"<c r="A1" t="inlineStr"><is><t>Olá &amp; &#x1F600;</t></is></c><c r="B1"><f>A1&lt;2</f></c>"#))).is_ok());
    let mut input = parts("");
    input[4].1 = format!(
        "<!DOCTYPE worksheet [<!ENTITY data SYSTEM 'file:///private'>]>{}",
        input[4].1
    );
    assert!(Document::parse(&archive(input)).is_err());
}
#[test]
fn declared_and_actual_inflated_sizes_both_remain_bounded() {
    let mut input = parts("");
    input[4].1.push_str(&" ".repeat(MAX_PART));
    assert!(Document::parse(&archive(input)).is_err());
    assert!(Document::parse(&[]).is_err());
    assert!(Document::parse(&vec![0; MAX_BYTES + 1]).is_err());
}
