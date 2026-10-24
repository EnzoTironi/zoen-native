//! Actual upstream PDF layout/fonts; host scope and accepted DOCX remain authoritative.
use super::{
    Accept, Action, ActorKind, Command, Export, Head, HostBridge, Mutation, Open, Proposal, Result,
    Scope, TextReplace, failure, hash, profile,
};
use serde_json::json;
#[path = "pdf_test_text.rs"]
mod text;
fn scope() -> Scope {
    Scope {
        artifact_id: "pdf-document".into(),
        actor_id: "pdf-owner".into(),
        actor_kind: ActorKind::Human,
    }
}
fn head(version: &str, bytes: &[u8]) -> Head {
    Head {
        version: version.into(),
        sha256: hash(bytes),
    }
}
fn pdf_request(head: Head) -> Result<Export> {
    serde_json::from_value(json!({"scope":scope(),"head":head,"format":"pdf"}))
        .map_err(|_| failure("PDF export is not exposed"))
}
fn contents(p: &Proposal) -> Result<String> {
    assert!(p.bytes.starts_with(b"%PDF-"));
    assert_eq!(p.metadata["format"], "pdf");
    assert_eq!(p.metadata["mime"], "application/pdf");
    assert_eq!(p.metadata["sha256"], hash(&p.bytes));
    assert_eq!(p.metadata["byteLength"], p.bytes.len());
    Ok(text::extract_text(&p.bytes)?
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" "))
}
#[test]
fn actual_pdf_keeps_original_before_ack_then_exports_accepted_and_selected_cold_history()
-> Result<()> {
    let original = profile::encode("Original histórico Olá office\nSecond paragraph")?;
    let first = head("1", &original);
    let mut bridge = HostBridge::open(
        Open {
            scope: scope(),
            head: first.clone(),
        },
        &original,
    )?;
    let before = bridge.status();
    let pdf = bridge.export(&pdf_request(first.clone())?)?;
    assert!(contents(&pdf)?.contains("Original histórico Olá office"));
    assert!(contents(&pdf)?.contains("Second paragraph"));
    assert_eq!(pdf.metadata["sourceHead"], json!(first));
    assert_eq!(bridge.status(), before);
    let proposal = bridge.execute(Command {
        mutation: Mutation {
            scope: scope(),
            base: first.clone(),
            mutation_id: "pdf-edit".into(),
        },
        action: Action::TextReplace(TextReplace {
            text: "Replacement accepted".into(),
        }),
    })?;
    let pending = bridge.status();
    let held = contents(&bridge.export(&pdf_request(first.clone())?)?)?;
    assert!(held.contains("Original histórico"));
    assert!(!held.contains("Replacement"));
    assert_eq!(bridge.status(), pending);
    let second = head("2", &proposal.bytes);
    bridge.accept(&Accept {
        scope: scope(),
        mutation_id: "pdf-edit".into(),
        head: second.clone(),
    })?;
    let latest = contents(&bridge.export(&pdf_request(second.clone())?)?)?;
    assert!(latest.contains("Replacement accepted"));
    assert!(!latest.contains("Original"));
    assert!(bridge.export(&pdf_request(first.clone())?).is_err());
    let cold = HostBridge::open(
        Open {
            scope: scope(),
            head: second.clone(),
        },
        &proposal.bytes,
    )?;
    assert!(contents(&cold.export(&pdf_request(second)?)?)?.contains("Replacement accepted"));
    let historical = HostBridge::open(
        Open {
            scope: scope(),
            head: first.clone(),
        },
        &original,
    )?;
    let selected = historical.export(&pdf_request(first)?)?;
    assert!(contents(&selected)?.contains("Original histórico"));
    assert!(!contents(&selected)?.contains("Replacement"));
    assert_eq!(
        historical.inspect()["text"],
        "Original histórico Olá office\nSecond paragraph"
    );
    Ok(())
}
#[test]
fn pdf_rejects_foreign_scope_head_and_unknown_format_without_changing_document() -> Result<()> {
    let bytes = profile::encode("Private accepted text")?;
    let h = head("7", &bytes);
    let bridge = HostBridge::open(
        Open {
            scope: scope(),
            head: h.clone(),
        },
        &bytes,
    )?;
    let before = bridge.status();
    let mut req = pdf_request(h.clone())?;
    req.scope.actor_id = "foreign-owner".into();
    assert!(bridge.export(&req).is_err());
    req.scope = scope();
    req.head.version = "8".into();
    assert!(bridge.export(&req).is_err());
    req.head = h.clone();
    req.head.sha256 = "0".repeat(64);
    assert!(bridge.export(&req).is_err());
    assert!(
        serde_json::from_value::<Export>(json!({"scope":scope(),"head":h,"format":"html"}))
            .is_err()
    );
    assert_eq!(bridge.status(), before);
    assert_eq!(bridge.inspect()["text"], "Private accepted text");
    Ok(())
}
