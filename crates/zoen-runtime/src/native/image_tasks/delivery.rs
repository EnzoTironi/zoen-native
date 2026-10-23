//! Delivery of an already-activated FileDoc. No model call or new document is
//! reachable here; the original capsule and Publication remain the authority.
mod storage;
use super::output::Publication;
use super::*;
use crate::execution::Fence;
use client_core::relay_output_transport::RelayOutputError;
use protocol::{ClientFrame, Envelope};
use types::Seen;

/// Operator routing, not a permission. The source cell and authenticated
/// Challenge must match it before any artifact bytes leave custody.
#[derive(Clone, Debug)]
pub struct ImageDeliveryTarget {
    pub(super) url: String,
    pub(super) relay: String,
    pub(super) source: String,
}
impl ImageDeliveryTarget {
    pub fn configured(url: String, relay: String, source: String) -> Result<Self, RuntimeError> {
        if url.len() > 2048
            || url.is_empty()
            || relay.is_empty()
            || relay.len() > 256
            || !id(&source)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(Self { url, relay, source })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageDeliveryProgress {
    CatchingUp { through: Seen },
    Accepted { item: String, accepted: Seen },
}

struct Delivery<'a> {
    runtime: &'a RuntimeAuthority,
    custody: &'a NativeCustody,
    capability: &'a FreshOutputNamespace,
    credential: &'a Credential,
    fence: &'a Fence,
    run: &'a str,
    record: Record,
    publication: Publication,
    publication_digest: String,
    envelope: Envelope,
    current: Root,
    core: Box<DeviceCore>,
    source_head: Seen,
}
pub(super) fn transport_error(error: RelayOutputError) -> RuntimeError {
    match error {
        RelayOutputError::Unknown => RuntimeError::RestoreUnreconciled,
        RelayOutputError::Invalid
        | RelayOutputError::Blob(client_core::blob_transport::BlobUploadError::Invalid) => {
            RuntimeError::InvalidBinding
        }
        RelayOutputError::Refused
        | RelayOutputError::Blob(client_core::blob_transport::BlobUploadError::Refused(_)) => {
            RuntimeError::Denied
        }
        _ => RuntimeError::Unavailable,
    }
}

impl NativeCustody {
    pub(crate) async fn deliver_image_output(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        target: &ImageDeliveryTarget,
        capability: &FreshOutputNamespace,
    ) -> Result<ImageDeliveryProgress, RuntimeError> {
        runtime.continuity()?;
        let source = runtime.execution.reply_source()?;
        if source.cell() != target.source {
            return Err(RuntimeError::InvalidBinding);
        }
        capability
            .check_in(&runtime.execution, &runtime.execution.transaction().await?)
            .await?;
        let (record, _) = runtime
            .execution
            .read_image_task(run, &runtime.custody)
            .await?;
        let principal = record.original.context.principal.clone();
        let credential = self.credential(&principal.agent, &principal.device)?;
        runtime.execution.collect_native_stage(&principal).await?;
        runtime.execution.collect_native_retired(&principal).await?;
        let fence = runtime.execution.acquire_native_device(&principal).await?;
        let result = Box::pin(async {
            let bytes = runtime
                .execution
                .transaction()
                .await?
                .get(&runtime.execution.run(run, "image-output/1"), false)
                .await?
                .ok_or(RuntimeError::Denied)?;
            let publication: Publication =
                runtime
                    .custody
                    .open(&format!("native-image-output/1/{run}"), None, &bytes)?;
            let frame =
                ClientFrame::decode(&publication.wire).map_err(|_| RuntimeError::InvalidBinding)?;
            if frame.encode() != publication.wire
                || publication.task != record.task.task_digest
                || publication.evidence
                    != runtime
                        .execution
                        .image_capture_digest(&record.task, &runtime.custody)
                        .await?
                || publication.item.is_empty()
                || !digest(&publication.sha256)
                || publication.generation <= record.original.context.generation
            {
                return Err(RuntimeError::InvalidBinding);
            }
            let ClientFrame::Publish { env: envelope } = frame else {
                return Err(RuntimeError::InvalidBinding);
            };
            if envelope.author() != principal.agent
                || envelope.device() != Some(principal.device.as_str())
                || envelope.cert.as_deref() != Some(credential.certificate.as_str())
                || envelope.space() != record.task.binding.context.space
                || envelope.verify().is_err()
                || envelope.is_pruned()
                || !envelope.is_sealed()
            {
                return Err(RuntimeError::InvalidBinding);
            }
            // Historical acceptance is observed from the actual source again.
            // It cannot authorize another upload or publication after revocation.
            if let Some(accepted) = &publication.accepted {
                let position = source
                    .position_in(
                        &runtime.execution.transaction().await?,
                        envelope.space(),
                        accepted.seq,
                    )
                    .await
                    .map_err(|_| RuntimeError::InvalidBinding)?;
                if position.env != envelope || position.hash != accepted.hash {
                    return Err(RuntimeError::InvalidBinding);
                }
                return Ok(ImageDeliveryProgress::Accepted {
                    item: publication.item,
                    accepted: accepted.clone(),
                });
            }
            let capsule = runtime
                .execution
                .read_native(&principal, Some(&fence))
                .await?;
            let current = capsule.root.clone();
            let core = Box::new(
                DeviceCore::restore(
                    self.open_image(&principal, capsule)?,
                    credential.unlocked()?,
                )
                .map_err(|_| RuntimeError::InvalidBinding)?,
            );
            let mut delivery = Delivery {
                runtime,
                custody: self,
                capability,
                credential,
                fence: &fence,
                run,
                record,
                publication,
                publication_digest: hash(&bytes),
                envelope,
                current,
                core,
                source_head: Seen {
                    seq: 0,
                    hash: String::new(),
                },
            };
            delivery.advance(target).await
        })
        .await;
        let released = runtime
            .execution
            .release_native_device(&principal, &fence)
            .await;
        match result {
            Err(error) => Err(error),
            Ok(progress) => {
                released?;
                Ok(progress)
            }
        }
    }
}

impl Delivery<'_> {
    async fn advance(
        &mut self,
        target: &ImageDeliveryTarget,
    ) -> Result<ImageDeliveryProgress, RuntimeError> {
        if let Some(progress) = self.catch_up().await? {
            return Ok(progress);
        }
        let file = self
            .core
            .image_output_file(
                &self.record.task.request_id,
                &self.record.task.approval_hash,
                &self.envelope,
                self.runtime.finance.reply_clock().await?,
            )
            .map_err(|_| RuntimeError::Denied)?;
        if file.sha256 != self.publication.sha256
            || file.chunks.len() != self.publication.blobs.len()
            || file
                .chunks
                .iter()
                .zip(&self.publication.blobs)
                .any(|(a, b)| a != &b.chunk)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        self.authorize().await?;
        let actor = event_log::Author::certified(
            self.credential.agent.id.clone(),
            event_log::Signer::from_secret(&self.credential.secret),
            self.credential.certificate.clone(),
        );
        let mut transport = self
            .runtime
            .connect_bound_image_output(
                &target.url,
                &target.relay,
                actor,
                &self.envelope,
                self.publication.checkpoint.as_ref(),
            )
            .await?;
        for index in 0..self.publication.blobs.len() {
            let blob = self.publication.blobs[index].chunk.blob.clone();
            if !self
                .core
                .image_output_upload_pending(&blob)
                .map_err(|_| RuntimeError::InvalidBinding)?
            {
                continue;
            }
            self.authorize().await?;
            let bytes =
                blobs::ciphertext(self.runtime, self.run, &self.publication.blobs[index]).await?;
            let receipt = transport
                .upload(&blob, bytes, self.runtime.finance.reply_clock().await?)
                .await
                .map_err(transport_error)?;
            // If the HTTP ACK is lost, this line is unreachable and the same
            // pending reference survives cold recovery. No empty payload/ACK.
            self.authorize().await?;
            self.core
                .acknowledge_image_upload(
                    &self.record.task.request_id,
                    &self.record.task.approval_hash,
                    &self.envelope,
                    &receipt,
                    self.runtime.finance.reply_clock().await?,
                )
                .map_err(|_| RuntimeError::Denied)?;
            self.activate().await?;
        }
        self.authorize().await?;
        self.core
            .image_output_ready(
                &self.record.task.request_id,
                &self.record.task.approval_hash,
                &self.envelope,
                self.runtime.finance.reply_clock().await?,
            )
            .map_err(|_| RuntimeError::Denied)?;
        let expected = self
            .runtime
            .image_publish_expectation(
                &target.relay,
                &self.envelope,
                self.publication.checkpoint.as_ref(),
            )
            .await?;
        let seq = match expected {
            Some(expected) => transport.publish_managed(&self.envelope, expected).await,
            None => transport.publish(&self.envelope).await,
        }
        .map_err(transport_error)?;
        let position = self
            .runtime
            .execution
            .reply_source()?
            .position_in(
                &self.runtime.execution.transaction().await?,
                self.envelope.space(),
                seq,
            )
            .await
            .map_err(|_| RuntimeError::InvalidBinding)?;
        if position.env != self.envelope {
            return Err(RuntimeError::InvalidBinding);
        }
        self.catch_up().await?.ok_or(RuntimeError::Unavailable)
    }

    async fn catch_up(&mut self) -> Result<Option<ImageDeliveryProgress>, RuntimeError> {
        let retained = self
            .core
            .retained_head(self.envelope.space())
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let batch = self
            .runtime
            .execution
            .reply_source()?
            .read_in(
                &self.runtime.execution.transaction().await?,
                self.envelope.space(),
                retained.as_ref().map(|h| (h.seq, h.hash.as_str())),
            )
            .await
            .map_err(|_| RuntimeError::Denied)?;
        self.source_head = Seen {
            seq: batch.head.seq(),
            hash: batch.head.hash().into(),
        };
        let mut accepted = None;
        for position in &batch.entries {
            if position.env.client_id() == self.envelope.client_id() {
                if position.env != self.envelope {
                    return Err(RuntimeError::InvalidBinding);
                }
                accepted = Some(Seen {
                    seq: position.seq,
                    hash: position.hash.clone(),
                });
            }
        }
        let changed = !batch.entries.is_empty();
        self.core
            .ingest(batch.entries)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        if let Some(accepted) = accepted {
            self.publication.accepted = Some(accepted.clone());
            self.activate().await?;
            return Ok(Some(ImageDeliveryProgress::Accepted {
                item: self.publication.item.clone(),
                accepted,
            }));
        }
        if changed {
            self.activate().await?;
        }
        let through = self
            .core
            .retained_head(self.envelope.space())
            .map_err(|_| RuntimeError::InvalidBinding)?
            .ok_or(RuntimeError::InvalidBinding)?;
        if through != self.source_head {
            return Ok(Some(ImageDeliveryProgress::CatchingUp { through }));
        }
        Ok(None)
    }
}
