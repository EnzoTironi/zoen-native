use super::image_task_cases::{approved, png};
use super::*;

#[test]
fn an_already_projected_image_requires_its_original_review_and_upload_ack() {
    let (mut f, request, approval) = approved().unwrap();
    let prepared = f
        .core
        .prepare_image_task_referenced(
            &request,
            &approval,
            &png(1024, 1024),
            crate::engine::now_ms(),
        )
        .unwrap();
    let envelope = prepared.envelope().clone();
    let core = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec()).unwrap(),
        f.credential(),
    )
    .unwrap();
    assert_eq!(
        core.image_output_file(&request, &approval, &envelope, crate::engine::now_ms())
            .unwrap(),
        *prepared.file()
    );
    assert!(matches!(
        core.image_output_ready(&request, &approval, &envelope, crate::engine::now_ms()),
        Err(NativeError::NotReady)
    ));
    assert!(matches!(
        core.image_output_file(
            &request,
            &"a".repeat(64),
            &envelope,
            crate::engine::now_ms()
        ),
        Err(NativeError::Permission)
    ));
    let mut other = envelope.clone();
    other.sig = "a".repeat(128);
    assert!(core
        .image_output_file(&request, &approval, &other, crate::engine::now_ms())
        .is_err());
    for chunk in &prepared.file().chunks {
        assert!(core.image_output_upload_pending(&chunk.blob).unwrap());
    }
}

#[test]
fn revoked_trust_refuses_a_prepared_image_without_changing_its_upload_barrier(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut f, request, approval) = approved().unwrap();
    let prepared = f
        .core
        .prepare_image_task_referenced(
            &request,
            &approval,
            &png(1024, 1024),
            crate::engine::now_ms(),
        )
        .unwrap();
    let envelope = prepared.envelope().clone();
    let chunks = prepared.file().chunks.clone();
    f.encrypted(
        EventBody::GrantRevoked {
            grant: "image-act".into(),
        },
        crate::engine::now_ms(),
    )?;
    assert!(matches!(
        f.core
            .image_output_file(&request, &approval, &envelope, crate::engine::now_ms()),
        Err(NativeError::Permission)
    ));
    assert!(f
        .core
        .image_output_ready(&request, &approval, &envelope, crate::engine::now_ms())
        .is_err());
    for chunk in chunks {
        assert!(f.core.image_output_upload_pending(&chunk.blob).unwrap());
    }
    Ok(())
}
