use super::*;
use pdfcraft_cos::{Object, PdfString, SaveOptions, write_full};

fn source() -> Result<Vec<u8>> {
    let mut doc = pdfcraft_create::from_text(
        "Shared review",
        "Original paragraph.\n\nSecond paragraph.",
        (595.0, 842.0),
        16.0,
    )
    .map_err(|error| error.to_string())?;
    let root = doc.root().ok_or("missing_root")?;
    doc.update_dict(root, |dict| {
        dict.set(
            b"ZoenUnknown",
            Object::String(PdfString::text("preserve me")),
        );
    })
    .map_err(|error| error.to_string())?;
    write_full(&doc, &SaveOptions::default()).map_err(|error| error.to_string())
}
fn scope() -> Scope {
    Scope {
        artifact_id: "pdf-review".into(),
        actor_id: "reviewer".into(),
        actor_kind: ActorKind::Agent,
    }
}
fn head(version: &str, bytes: &[u8]) -> Head {
    Head {
        version: version.into(),
        sha256: hash(bytes),
    }
}
fn mutation(bytes: &[u8], id: &str) -> Mutation {
    Mutation {
        scope: scope(),
        base: head("1", bytes),
        mutation_id: id.into(),
    }
}

#[test]
fn actual_edit_requires_exact_receipt_preserves_unknown_and_selected_export() -> Result<()> {
    let source = source()?;
    let mut bridge = HostBridge::open(
        Open {
            scope: scope(),
            head: head("1", &source),
        },
        &source,
    )?;
    let command = Command {
        mutation: mutation(&source, "replace-1"),
        action: Operation::ReplaceText {
            page: 0,
            block: 0,
            expected: "Original paragraph.".into(),
            text: "Reviewed paragraph.".into(),
        },
    };
    let proposal = bridge.execute(command.clone())?;
    assert_eq!(
        proposal.metadata["operation"],
        json!({"kind":"command","action":command.action})
    );
    assert_ne!(proposal.bytes, source);
    assert_eq!(
        bridge.execute(command)?.bytes,
        proposal.bytes,
        "retry must retain one proposal"
    );
    assert_eq!(
        bridge
            .export(&Export {
                scope: scope(),
                head: head("1", &source),
                format: ExportFormat::Pdf
            })?
            .bytes,
        source
    );
    let wrong = Accept {
        scope: scope(),
        mutation_id: "replace-1".into(),
        head: head("3", &proposal.bytes),
    };
    assert!(bridge.accept(&wrong).is_err());
    assert_eq!(bridge.status()["head"]["sha256"], hash(&source));
    let receipt = Accept {
        head: head("2", &proposal.bytes),
        ..wrong
    };
    bridge.accept(&receipt)?;
    bridge.accept(&receipt)?;
    assert_eq!(
        bridge
            .export(&Export {
                scope: scope(),
                head: receipt.head,
                format: ExportFormat::Pdf
            })?
            .bytes,
        proposal.bytes
    );
    let (session, id) = profile::open(&proposal.bytes)?;
    assert!(
        session
            .get(id)
            .ok_or("missing_document")?
            .text_blocks(0)
            .iter()
            .any(|block| block.text == "Reviewed paragraph.")
    );
    let cos = pdfcraft_cos::Document::open(std::sync::Arc::new(proposal.bytes))
        .map_err(|error| error.to_string())?;
    let root = cos.get(cos.root().ok_or("missing_root")?);
    assert_eq!(
        root.as_dict()
            .and_then(|dict| dict.get(b"ZoenUnknown"))
            .and_then(Object::as_string)
            .map(PdfString::to_text)
            .as_deref(),
        Some("preserve me")
    );
    Ok(())
}

#[test]
fn stale_paragraph_and_forged_owner_cannot_propose_or_mutate_accepted_bytes() -> Result<()> {
    let source = source()?;
    let mut bridge = HostBridge::open(
        Open {
            scope: scope(),
            head: head("1", &source),
        },
        &source,
    )?;
    let mut command = Command {
        mutation: mutation(&source, "replace-1"),
        action: Operation::ReplaceText {
            page: 0,
            block: 0,
            expected: "Wrong source paragraph.".into(),
            text: "Reviewed paragraph.".into(),
        },
    };
    assert_eq!(
        bridge.execute(command.clone()).err().as_deref(),
        Some("paragraph_changed")
    );
    command.mutation.scope.artifact_id = "other-artifact".into();
    assert_eq!(
        bridge.execute(command).err().as_deref(),
        Some("scope_changed")
    );
    assert!(bridge.status()["pendingMutationId"].is_null());
    assert_eq!(bridge.accepted, source);
    Ok(())
}

#[test]
fn structural_parser_refuses_repair_and_live_javascript_is_never_enabled() -> Result<()> {
    let source = source()?;
    let (session, _) = profile::open(&source)?;
    assert!(!session.javascript());
    let mut truncated = source.clone();
    truncated.truncate(truncated.len().saturating_sub(16));
    assert!(profile::checked(&truncated).is_err());
    let mut prefixed = b"arbitrary prefix".to_vec();
    prefixed.extend(source);
    assert!(structure::parse(&prefixed).is_err());
    Ok(())
}

#[test]
fn repeated_real_edits_compact_without_losing_unknown_data_or_selected_versions() -> Result<()> {
    let source = source()?;
    let mut bridge = HostBridge::open(
        Open {
            scope: scope(),
            head: head("1", &source),
        },
        &source,
    )?;
    let mut compactions = 0;
    let mut preceding_revisions = 1;
    for index in 1..=70 {
        let command = Command {
            mutation: Mutation {
                scope: scope(),
                base: bridge.head.clone(),
                mutation_id: format!("rotate-{index}"),
            },
            action: Operation::RotatePages {
                pages: vec![0],
                degrees: 90,
            },
        };
        let proposal = bridge.execute(command)?;
        let cos = pdfcraft_cos::Document::open(std::sync::Arc::new(proposal.bytes.clone()))
            .map_err(|error| error.to_string())?;
        let revisions = cos.revisions().len();
        if revisions < preceding_revisions {
            compactions += 1;
        }
        preceding_revisions = revisions;
        assert!(
            revisions <= 32,
            "ordinary edits must never reach the admission revision limit"
        );
        let catalog = cos.get(cos.root().ok_or("missing_root")?);
        assert_eq!(
            catalog
                .as_dict()
                .and_then(|dict| dict.get(b"ZoenUnknown"))
                .and_then(Object::as_string)
                .map(PdfString::to_text)
                .as_deref(),
            Some("preserve me")
        );
        let receipt = Accept {
            scope: scope(),
            mutation_id: format!("rotate-{index}"),
            head: head(&(index + 1).to_string(), &proposal.bytes),
        };
        bridge.accept(&receipt)?;
        assert_eq!(
            bridge
                .export(&Export {
                    scope: scope(),
                    head: receipt.head,
                    format: ExportFormat::Pdf
                })?
                .bytes,
            proposal.bytes
        );
    }
    assert!(compactions >= 2);
    Ok(())
}

#[test]
fn image_codec_refuses_false_pdf_dimensions_and_bounded_decode_refuses_expansion() -> Result<()> {
    use pdfcraft_cos::{Dict, Stream};
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode(&[30; 18], 3, 2, image::ExtendedColorType::Rgb8)
        .map_err(|error| error.to_string())?;
    let mut doc = pdfcraft_create::from_images(&[("Original fixture.jpg".into(), jpeg)])
        .map_err(|error| error.to_string())?;
    let image = doc
        .scan_objects_checked()
        .find_map(|(reference, object)| {
            object
                .ok()
                .filter(|object| {
                    object
                        .as_dict()
                        .is_some_and(|dict| dict.name(b"Subtype") == Some(b"Image"))
                })
                .map(|_| reference)
        })
        .ok_or("missing_image")?;
    doc.update_dict(image, |dict| dict.set(b"Width", Object::Int(1)))
        .map_err(|error| error.to_string())?;
    let bytes = write_full(&doc, &SaveOptions::default()).map_err(|error| error.to_string())?;
    assert!(
        structure::parse(&bytes).is_ok(),
        "Core checks declared scalars, not image headers"
    );
    assert_eq!(
        profile::open(&bytes).err().as_deref(),
        Some("image_dimensions_changed")
    );
    let mut doc = pdfcraft_create::blank(595.0, 842.0, 1).map_err(|error| error.to_string())?;
    let payload = doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        &vec![0; (8 << 20) + 1],
    )));
    doc.update_dict(doc.root().ok_or("missing_root")?, |dict| {
        dict.set(b"ZoenOpaquePayload", Object::Ref(payload));
    })
    .map_err(|error| error.to_string())?;
    let bytes = write_full(&doc, &SaveOptions::default()).map_err(|error| error.to_string())?;
    assert!(structure::parse(&bytes).is_ok());
    assert_eq!(
        profile::open(&bytes).err().as_deref(),
        Some("unsupported_or_oversized_stream")
    );
    Ok(())
}
