use super::*;
use std::io::{Cursor, Write};

fn scope() -> Scope {
    Scope {
        artifact_id: "fixture-document".into(),
        actor_id: "fixture-owner".into(),
        actor_kind: ActorKind::Agent,
    }
}
fn head(version: &str, bytes: &[u8]) -> Head {
    Head {
        version: version.into(),
        sha256: hash(bytes),
    }
}
fn opened(text: &str) -> Result<(HostBridge, Vec<u8>)> {
    let bytes = profile::encode(text)?;
    Ok((
        HostBridge::open(
            Open {
                scope: scope(),
                head: head("1", &bytes),
            },
            &bytes,
        )?,
        bytes,
    ))
}
fn command(base: Head, id: &str, text: &str) -> Command {
    Command {
        mutation: Mutation {
            scope: scope(),
            base,
            mutation_id: id.into(),
        },
        action: Action::TextReplace(TextReplace { text: text.into() }),
    }
}
fn exported(bridge: &HostBridge, head: Head) -> Result<Proposal> {
    bridge.export(&Export {
        scope: scope(),
        head,
        format: ExportFormat::Docx,
    })
}
fn with_extra(bytes: &[u8], extra: &str, content: &[u8]) -> Result<Vec<u8>> {
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| failure("fixture_zip"))?;
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut entry = input.by_index(i).map_err(|_| failure("fixture_zip"))?;
        writer
            .start_file(
                entry.name(),
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .map_err(|_| failure("fixture_zip"))?;
        std::io::copy(&mut entry, &mut writer).map_err(|_| failure("fixture_zip"))?;
    }
    writer
        .start_file(
            extra,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|_| failure("fixture_zip"))?;
    writer
        .write_all(content)
        .map_err(|_| failure("fixture_zip"))?;
    Ok(writer
        .finish()
        .map_err(|_| failure("fixture_zip"))?
        .into_inner())
}

#[test]
fn actual_engine_proposal_ack_and_cold_docx_preserve_unicode_and_selected_version() -> Result<()> {
    let (mut bridge, original) = opened("Original\nOlá 👩🏽‍💻")?;
    let base = head("1", &original);
    let request = command(base.clone(), "edit-1", "Novo\n日本語\t👨‍👩‍👧‍👦\n");
    let proposal = bridge.execute(request.clone())?;
    assert_eq!(
        profile::parse(&proposal.bytes)?.plain_text(StoryRef::Body),
        "Novo\n日本語\t👨‍👩‍👧‍👦\n"
    );
    assert_eq!(exported(&bridge, base.clone())?.bytes, original);
    assert_eq!(bridge.inspect()?["text"], "Original\nOlá 👩🏽‍💻");
    assert_eq!(bridge.execute(request.clone())?.bytes, proposal.bytes);
    let accepted = head("2", &proposal.bytes);
    bridge.accept(&Accept {
        scope: scope(),
        mutation_id: "edit-1".into(),
        head: accepted.clone(),
    })?;
    let replay = bridge.execute(request)?;
    assert_eq!(replay.bytes, proposal.bytes);
    assert_eq!(replay.metadata["acceptedHead"], json!(accepted));
    assert!(exported(&bridge, base).is_err());
    let cold = HostBridge::open(
        Open {
            scope: scope(),
            head: accepted.clone(),
        },
        &proposal.bytes,
    )?;
    assert_eq!(exported(&cold, accepted)?.bytes, proposal.bytes);
    Ok(())
}

#[test]
fn pending_and_accepted_mutation_ids_cannot_change_intent_or_scope() -> Result<()> {
    let (mut bridge, bytes) = opened("One")?;
    let base = head("1", &bytes);
    let proposal = bridge.execute(command(base.clone(), "same-id", "Two"))?;
    assert_eq!(
        bridge
            .execute(command(base.clone(), "same-id", "Different"))
            .err(),
        Some(failure("proposal_pending"))
    );
    assert_eq!(
        bridge
            .execute(command(base.clone(), "new-id", "Three"))
            .err(),
        Some(failure("proposal_pending"))
    );
    let mut foreign = command(base.clone(), "same-id", "Two");
    foreign.mutation.scope.actor_id = "different-owner".into();
    assert_eq!(
        bridge.execute(foreign).err(),
        Some(failure("scope_changed"))
    );
    bridge.accept(&Accept {
        scope: scope(),
        mutation_id: "same-id".into(),
        head: head("2", &proposal.bytes),
    })?;
    assert_eq!(
        bridge.execute(command(base, "same-id", "Different")).err(),
        Some(failure("mutation_reused"))
    );
    Ok(())
}

#[test]
fn wrong_ack_and_stale_base_do_not_advance_accepted_document() -> Result<()> {
    let (mut bridge, bytes) = opened("Accepted")?;
    let base = head("1", &bytes);
    assert_eq!(
        bridge
            .execute(command(head("0", &bytes), "old", "No"))
            .err(),
        Some(failure("stale_head"))
    );
    let proposal = bridge.execute(command(base.clone(), "one", "Proposed"))?;
    for receipt in [
        Accept {
            scope: scope(),
            mutation_id: "other".into(),
            head: head("2", &proposal.bytes),
        },
        Accept {
            scope: scope(),
            mutation_id: "one".into(),
            head: head("3", &proposal.bytes),
        },
        Accept {
            scope: scope(),
            mutation_id: "one".into(),
            head: head("2", &bytes),
        },
    ] {
        assert_eq!(
            bridge.accept(&receipt).err(),
            Some(failure("receipt_mismatch"))
        );
        assert_eq!(exported(&bridge, base.clone())?.bytes, bytes);
    }
    Ok(())
}

#[test]
fn lenient_reader_extra_parts_are_refused_and_rich_document_is_preserved() -> Result<()> {
    let original = profile::encode("Text")?;
    for (name, data) in [
        ("word/vbaProject.bin", b"macro".as_slice()),
        ("customXml/item1.xml", b"<secret/>".as_slice()),
        ("../escape", b"x".as_slice()),
    ] {
        assert!(
            profile::parse(&with_extra(&original, name, data)?).is_err(),
            "must reject {name}"
        );
    }
    let mut rich = wordcraft_doc::Document::from_text("Text");
    rich.core.title = "Preserved document metadata".into();
    let rich_bytes = wordcraft_docx::write(&rich).map_err(|_| failure("fixture_docx"))?;
    assert_eq!(profile::parse(&rich_bytes)?, rich);
    // ZipWriter rejects duplicate names. Forge equal-length local/central names in a
    // genuine writer package so the parser, rather than the fixture builder, refuses it.
    let mut reader =
        zip::ZipArchive::new(Cursor::new(&original)).map_err(|_| failure("fixture_zip"))?;
    let mut canonical_xml = Vec::new();
    std::io::Read::read_to_end(
        &mut reader
            .by_name("word/document.xml")
            .map_err(|_| failure("fixture_zip"))?,
        &mut canonical_xml,
    )
    .map_err(|_| failure("fixture_zip"))?;
    let mut duplicate = with_extra(&original, "word/forgedxx.xml", &canonical_xml)?;
    let old = b"word/forgedxx.xml";
    let new = b"word/document.xml";
    assert_eq!(old.len(), new.len());
    let mut replaced = 0;
    for index in 0..=duplicate.len().saturating_sub(old.len()) {
        if &duplicate[index..index + old.len()] == old {
            duplicate[index..index + old.len()].copy_from_slice(new);
            replaced += 1;
        }
    }
    assert_eq!(replaced, 2);
    assert!(profile::parse(&duplicate).is_err());
    Ok(())
}

#[test]
fn zip_declared_size_count_and_actual_text_bounds_refuse_without_partial_edit() -> Result<()> {
    let original = profile::encode("Small")?;
    let bomb = with_extra(&original, "huge.xml", &vec![b' '; 1024 * 1024 + 1])?;
    assert_eq!(profile::parse(&bomb).err(), Some(failure("inflated_size")));
    let mut many = original.clone();
    for i in 0..33 {
        many = with_extra(&many, &format!("extra{i}.xml"), b"x")?;
    }
    assert_eq!(profile::parse(&many).err(), Some(failure("part_count")));
    let (mut bridge, bytes) = opened("Small")?;
    let base = head("1", &bytes);
    for text in [
        "x".repeat(profile::MAX_TEXT + 1),
        "bad\rline".into(),
        "bad\0text".into(),
        "before\u{fffe}after".into(),
        "before\u{ffff}after".into(),
    ] {
        assert_eq!(
            bridge
                .execute(command(base.clone(), "invalid", &text))
                .err(),
            Some(failure("unsupported_text"))
        );
    }
    assert_eq!(exported(&bridge, base)?.bytes, bytes);
    assert!(bridge.pending.is_none());
    Ok(())
}

#[test]
fn command_schema_is_closed_and_version_overflow_cannot_create_proposal() -> Result<()> {
    for action in [
        json!({"command":"file.open","params":{"path":"/tmp/private"}}),
        json!({"command":"text.replace","params":{"text":"ok","path":"forbidden"}}),
    ] {
        assert!(serde_json::from_value::<Action>(action).is_err());
    }
    let bytes = profile::encode("")?;
    let base = head(&u64::MAX.to_string(), &bytes);
    let mut bridge = HostBridge::open(
        Open {
            scope: scope(),
            head: base.clone(),
        },
        &bytes,
    )?;
    assert_eq!(
        bridge.execute(command(base, "overflow", "No")).err(),
        Some(failure("invalid_mutation"))
    );
    assert!(bridge.pending.is_none());
    Ok(())
}

#[test]
fn actual_writer_xml_boundary_scalars_and_whitespace_survive_proposal_and_cold_reopen() -> Result<()>
{
    for text in [
        "<&>\"' and entities",
        "\t\t\n\ntrailing\t\n",
        "boundary\u{d7ff}\u{e000}\u{fffd}\u{10000}\u{10ffff}",
        "valid XML noncharacters in other planes: \u{1fffe}\u{1ffff}\u{10fffe}",
        "Olá 日本語 👩🏽‍💻 👨‍👩‍👧‍👦",
    ] {
        let (mut bridge, original) = opened("Unchanged")?;
        let request = command(head("1", &original), "boundary-edit", text);
        let proposal = bridge.execute(request)?;
        let cold = HostBridge::open(
            Open {
                scope: scope(),
                head: head("2", &proposal.bytes),
            },
            &proposal.bytes,
        )?;
        assert_eq!(cold.inspect()?["text"], text);
        assert_eq!(exported(&bridge, head("1", &original))?.bytes, original);
    }
    Ok(())
}

#[path = "inspect_tests.rs"]
mod inspect;
