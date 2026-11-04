//! Rebuild only the reference to an authenticated original publication during
//! existing native activation recovery. This does not infer, sign or publish.
use super::*;
use protocol::{ClientFrame, Envelope};
use relay_admission::managed_images::Checkpoint;

pub(in crate::native) struct ActivationPublication<'a> {
    pub(in crate::native) run: &'a str,
    pub(in crate::native) bytes: &'a [u8],
    pub(in crate::native) envelope: &'a Envelope,
    pub(in crate::native) target: &'a Root,
    pub(in crate::native) checkpoint: &'a Checkpoint,
}

pub(in crate::native) async fn recover_activation_reference(
    runtime: &RuntimeAuthority,
    transaction: &Transaction,
    publication: ActivationPublication<'_>,
) -> Result<(), RuntimeError> {
    let ActivationPublication {
        run,
        bytes,
        envelope,
        target,
        checkpoint,
    } = publication;
    let (record, original) = runtime
        .execution
        .reply_in(transaction, run, &runtime.custody)
        .await?;
    let capture = runtime
        .execution
        .output_capture_in(transaction, &record, &original, &runtime.custody)
        .await?;
    let publication: SealedRecord<PublicationRecord> = open_metadata(
        &runtime.custody,
        &format!("native-publication/1/{run}"),
        bytes,
    )?;
    let value = &publication.value;
    value.validate(&record, &original, &capture.association.digest)?;
    if value.root != *target
        || value.checkpoint.as_ref() != Some(checkpoint)
        || value.client_id != envelope.client_id()
        || value.publish
            != hash(
                &(ClientFrame::Publish {
                    env: envelope.clone(),
                })
                .encode(),
            )
    {
        return Err(RuntimeError::InvalidBinding);
    }
    let reference = runtime
        .execution
        .native_references(target)
        .pack(&("publication/1", run));
    let prior = transaction.get(&reference, false).await?;
    if prior
        .as_deref()
        .is_some_and(|value| value != target.manifest_digest.as_bytes())
    {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    transaction.set(&reference, target.manifest_digest.as_bytes());
    Ok(())
}
