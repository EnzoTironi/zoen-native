use crate::{Operation, managed::*};
use photocraft_doc::{ColorMode, Document, Layer, Rect, SampleType, Size};
use photocraft_format::SaveOptions;
use serde_json::json;
use sha2::Digest;

pub(crate) fn seed() -> Result<Vec<u8>> {
    let mut doc = Document::new(
        "Owned image",
        Size::new(320, 240),
        ColorMode::Rgb,
        SampleType::U8,
    );
    let mut layer = Layer::raster("Image", doc.pixel_format());
    let surface = layer.surface_mut().ok_or("surface_missing")?;
    surface.fill_rect(Rect::new(0, 0, 160, 240), &[1.0, 0.0, 0.0, 1.0]);
    surface.fill_rect(Rect::new(160, 0, 320, 240), &[0.0, 0.0, 1.0, 1.0]);
    doc.layers.push(layer);
    photocraft_format::save_to_bytes(&doc, &SaveOptions::default())
        .map_err(|_| failure("seed_encoding"))
}
pub(crate) fn scope() -> Scope {
    Scope {
        artifact_id: "image-fixture".into(),
        actor_id: "human-fixture".into(),
        actor_kind: ActorKind::Human,
    }
}
pub(crate) fn head(version: &str, bytes: &[u8]) -> Head {
    Head {
        version: version.into(),
        sha256: hash(bytes),
    }
}
pub(crate) fn mutation(bytes: &[u8], id: &str) -> Mutation {
    Mutation {
        scope: scope(),
        base: head("1", bytes),
        mutation_id: id.into(),
    }
}
#[test]
fn actual_worker_proposal_exact_ack_cold_reopen_and_accepted_png() -> Result<()> {
    let source = seed()?;
    let mut host = HostBridge::open(
        Open {
            scope: scope(),
            head: head("1", &source),
        },
        &source,
    )?;
    let command = Command {
        mutation: mutation(&source, "flip"),
        action: Operation::FlipHorizontal,
    };
    let p = host.execute(command.clone())?;
    assert_eq!(host.execute(command)?.bytes, p.bytes);
    assert_eq!(
        host.export(&Export {
            scope: scope(),
            head: head("1", &source),
            format: ExportFormat::Pcraft
        })?
        .bytes,
        source
    );
    let receipt = Accept {
        scope: scope(),
        mutation_id: "flip".into(),
        head: head("2", &p.bytes),
    };
    host.accept(&receipt)?;
    host.accept(&receipt)?;
    let cold = HostBridge::open(
        Open {
            scope: scope(),
            head: receipt.head.clone(),
        },
        &p.bytes,
    )?;
    let png = cold.export(&Export {
        scope: scope(),
        head: receipt.head,
        format: ExportFormat::Png,
    })?;
    assert_eq!(png.metadata["sourceHead"], json!(cold.status()["head"]));
    assert_eq!(png.metadata["format"], "png");
    assert_eq!(
        png.metadata["engineRevision"],
        cold.status()["engineRevision"]
    );
    assert_eq!(png.metadata["sha256"], hash(&png.bytes));
    assert_eq!(png.metadata["byteLength"], png.bytes.len());
    let image = photocraft_codecs::decode(&png.bytes).map_err(|_| failure("png_decode"))?;
    let rgba = image.to_rgba8();
    assert_eq!(&rgba[..4], &[0, 0, 255, 255]);
    assert_eq!(&rgba[319 * 4..320 * 4], &[255, 0, 0, 255]);
    assert_eq!(
        crate::inspect(&p.bytes, &sha2::Sha256::digest(&p.bytes).into())
            .map_err(|_| failure("native_reopen"))?
            .width,
        320
    );
    Ok(())
}
#[test]
fn old_or_foreign_ack_cannot_consume_next_original_proposal() -> Result<()> {
    let bytes = seed()?;
    let mut host = HostBridge::open(
        Open {
            scope: scope(),
            head: head("1", &bytes),
        },
        &bytes,
    )?;
    let first = host.execute(Command {
        mutation: mutation(&bytes, "first"),
        action: Operation::FlipHorizontal,
    })?;
    let receipt = Accept {
        scope: scope(),
        mutation_id: "first".into(),
        head: head("2", &first.bytes),
    };
    let mut wrong = receipt.clone();
    wrong.head.version = "3".into();
    assert!(host.accept(&wrong).is_err());
    wrong = receipt.clone();
    wrong.scope.actor_id = "other".into();
    assert!(host.accept(&wrong).is_err());
    host.accept(&receipt)?;
    let command = Command {
        mutation: Mutation {
            scope: scope(),
            base: receipt.head.clone(),
            mutation_id: "second".into(),
        },
        action: Operation::Rotate180,
    };
    let second = host.execute(command.clone())?;
    assert!(host.accept(&receipt).is_err());
    assert_eq!(host.execute(command)?.bytes, second.bytes);
    assert_eq!(host.status()["pendingMutationId"], "second");
    Ok(())
}
#[test]
fn changed_scope_base_hash_and_unknown_operations_fail_closed() -> Result<()> {
    let bytes = seed()?;
    let mut open = Open {
        scope: scope(),
        head: head("1", &bytes),
    };
    open.head.sha256 = "0".repeat(64);
    assert!(HostBridge::open(open, &bytes).is_err());
    let mut host = HostBridge::open(
        Open {
            scope: scope(),
            head: head("1", &bytes),
        },
        &bytes,
    )?;
    let mut m = mutation(&bytes, "stale");
    m.base.version = "0".into();
    assert!(
        host.execute(Command {
            mutation: m,
            action: Operation::FlipVertical
        })
        .is_err()
    );
    assert!(
        serde_json::from_value::<Command>(
            json!({"mutation":mutation(&bytes,"external"),"action":"file.open"})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<Open>(
            json!({"scope":scope(),"head":head("1",&bytes),"url":"external"})
        )
        .is_err()
    );
    assert!(host.status()["pendingMutationId"].is_null());
    Ok(())
}
