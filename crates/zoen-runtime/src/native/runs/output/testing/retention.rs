use super::*;

/// Real retained roots/reference rows, never a manufactured receipt or deadline.
pub(crate) async fn retention_rows(
    runtime: &RuntimeAuthority,
    run: &str,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, RuntimeError> {
    let tx = runtime.execution.transaction().await?;
    let (record, original) = runtime
        .execution
        .reply_in(&tx, run, &runtime.custody)
        .await?;
    let capture = runtime
        .execution
        .output_capture_in(&tx, &record, &original, &runtime.custody)
        .await?;
    let publication = runtime
        .execution
        .publication_in(
            &tx,
            &record,
            &original,
            &capture.association.digest,
            &runtime.custody,
        )
        .await?
        .ok_or(RuntimeError::Denied)?;
    let mut rows = Vec::new();
    for (root, reference) in [
        (
            &record.original,
            runtime
                .execution
                .native_references(&record.original)
                .pack(&(run,)),
        ),
        (
            &publication.value.root,
            runtime
                .execution
                .native_references(&publication.value.root)
                .pack(&("publication/1", run)),
        ),
    ] {
        rows.push((
            runtime.execution.root.pack(&(
                "native-image",
                root.context.principal.agent.as_str(),
                root.context.principal.device.as_str(),
                root.context.capsule.as_str(),
                "manifest",
            )),
            reference,
        ));
    }
    Ok(rows)
}

pub(crate) async fn collect_reply_images(
    runtime: &RuntimeAuthority,
    run: &str,
) -> Result<(), RuntimeError> {
    let (record, _) = runtime.execution.read_reply(run, &runtime.custody).await?;
    runtime
        .collect_reply_retired(&record.original.context.principal)
        .await
}
