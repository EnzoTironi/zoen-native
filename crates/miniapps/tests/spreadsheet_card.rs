//! A spreadsheet's portable card follows the same content-only contract as other Apps.
use localization::Lang;
use miniapps::{binary::gridcraft_registration::EMPTY, gridcraft::Document, headline, snapshot};
use serde_json::{json, Value};

#[test]
fn actual_workbook_metadata_projects_to_an_identifiable_private_card() {
    let _language = localization::enter(Lang::En);
    let document = Document::parse(EMPTY).expect("pinned engine-generated XLSX seed");
    let mut state: Value =
        serde_json::from_str(&document.metadata("Budget")).expect("validated metadata");
    state["cellContents"] = json!("private values that do not belong in a card");
    state["previewUrl"] = json!("https://untrusted.invalid/preview");
    state["deepLink"] = json!("zoen://app/other");
    let original = state.clone();
    let card = snapshot("gridcraft", &state, "it_workbook", 0).expect("spreadsheet card");
    assert_eq!(card["template"], "caption");
    assert_eq!(card["title"], "Budget");
    assert_eq!(card["eyebrow"], "Spreadsheet");
    assert_eq!(card["symbol"], "tablecells");
    assert_eq!(card["detail"], "1 sheet");
    assert_eq!(card["id"], "it_workbook");
    assert_eq!(card["appId"], "gridcraft");
    assert_eq!(card["deepLink"], "zoen://app/it_workbook");
    assert!(card.get("cellContents").is_none());
    assert!(card.get("previewUrl").is_none());
    assert!(!card.to_string().contains("http"));
    assert_eq!(state, original, "projection never rewrites the document");
    assert!(snapshot("unregistered-editor", &state, "it_workbook", 0).is_none());
}

#[test]
fn sheet_counts_localize_without_exporting_cell_values_or_metrics() {
    for (language, cases) in [
        (
            Lang::En,
            [(0, "0 sheets"), (1, "1 sheet"), (16, "16 sheets")],
        ),
        (Lang::Pt, [(0, "0 abas"), (1, "1 aba"), (16, "16 abas")]),
    ] {
        let _language = localization::enter(language);
        for (count, expected) in cases {
            let state = json!({"title":"Budget", "sheets":count, "cells":40_000});
            let (label, metrics) = headline("gridcraft", &state, 0);
            assert_eq!(label, expected);
            assert!(metrics.is_empty());
            let card = snapshot("gridcraft", &state, "it_workbook", 0).expect("card");
            assert_eq!(card["detail"], expected);
            assert!(card.get("cells").is_none());
        }
    }
}
