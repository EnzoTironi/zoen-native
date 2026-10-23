//! One original review, staged and published by the runtime-owned native device.
mod storage;
#[cfg(test)]
pub(crate) mod testing;
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use client_core::image_tasks::ImageTaskInput;
use protocol::{ClientFrame, Envelope};
use types::Seen;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageReviewProgress {
    CatchingUp {
        through: Seen,
    },
    Published {
        request: String,
        content_hash: String,
        accepted: Seen,
    },
}

/// Unprivileged locators and requested content. No native or financial facts.
pub struct ImageReviewIntent {
    pub agent: String,
    pub device: String,
    pub space: String,
    pub source: String,
    pub input: ImageTaskInput,
}
impl RuntimeAuthority {
    /// Stage the runtime device's own original, not a second Client's MLS state.
    /// Existing production continuity and output admission remain mandatory.
    pub async fn stage_image_task_review(
        &self,
        intent: ImageReviewIntent,
        target: super::delivery::ImageDeliveryTarget,
    ) -> Result<ImageReviewProgress, RuntimeError> {
        if ![&intent.agent, &intent.device, &intent.space, &intent.source]
            .into_iter()
            .all(|v| id(v))
        {
            return Err(RuntimeError::InvalidBinding);
        }
        self.continuity()?;
        let capability = self.production_output_scope()?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .stage_image_review(self, &intent, &target, capability)
            .await
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    source: String,
    input: String,
    request: String,
    content_hash: String,
    wire: String,
    accepted: Option<Seen>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checkpoint: Option<relay_admission::managed_images::Checkpoint>,
}
impl Review {
    fn envelope(
        &self,
        source: &str,
        input: &str,
        credential: &Credential,
        space: &str,
    ) -> Result<Envelope, RuntimeError> {
        if self.source != source
            || self.input != input
            || !id(&self.request)
            || !digest(&self.content_hash)
            || self.wire.len() > 90 * 1024
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let bytes = STANDARD
            .decode(&self.wire)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let frame = ClientFrame::decode(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if frame.encode() != bytes || STANDARD.encode(&bytes) != self.wire {
            return Err(RuntimeError::InvalidBinding);
        }
        let ClientFrame::Publish { env } = frame else {
            return Err(RuntimeError::InvalidBinding);
        };
        if env.author() != credential.agent.id
            || env.device() != Some(credential.principal().device.as_str())
            || env.cert.as_deref() != Some(credential.certificate.as_str())
            || env.space() != space
            || !env.is_sealed()
            || env.is_pruned()
            || env.verify().is_err()
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(env)
    }
    fn progress(&self) -> Result<ImageReviewProgress, RuntimeError> {
        Ok(ImageReviewProgress::Published {
            request: self.request.clone(),
            content_hash: self.content_hash.clone(),
            accepted: self.accepted.clone().ok_or(RuntimeError::Unavailable)?,
        })
    }
}
struct Staging<'a> {
    runtime: &'a RuntimeAuthority,
    custody: &'a NativeCustody,
    capability: &'a FreshOutputNamespace,
    credential: &'a Credential,
    fence: &'a crate::execution::Fence,
    key: Vec<u8>,
    prior: Option<String>,
    review: Option<Review>,
    envelope: Option<Envelope>,
    root: Root,
    core: Box<DeviceCore>,
    space: &'a str,
    head: Seen,
}
impl NativeCustody {
    pub(crate) async fn stage_image_review(
        &self,
        runtime: &RuntimeAuthority,
        intent: &ImageReviewIntent,
        target: &super::delivery::ImageDeliveryTarget,
        capability: &FreshOutputNamespace,
    ) -> Result<ImageReviewProgress, RuntimeError> {
        let ImageReviewIntent {
            agent,
            device,
            space,
            source,
            input,
        } = intent;
        runtime.continuity()?;
        if runtime.execution.reply_source()?.cell() != target.source {
            return Err(RuntimeError::InvalidBinding);
        }
        input.binding().map_err(|_| RuntimeError::InvalidBinding)?;
        let input_digest =
            hash(&serde_json::to_vec(&input).map_err(|_| RuntimeError::InvalidBinding)?);
        let credential = self.credential(agent, device)?;
        let principal = credential.principal();
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        runtime.execution.collect_native_stage(&principal).await?;
        runtime.execution.collect_native_retired(&principal).await?;
        let fence = runtime.execution.acquire_native_device(&principal).await?;
        let result = Box::pin(async {
            let key = runtime
                .execution
                .root
                .pack(&("native-image-review/1", agent, space, source));
            let trx = runtime.execution.transaction().await?;
            capability.check_in(&runtime.execution, &trx).await?;
            let bytes = trx.get(&key, false).await?;
            let prior = bytes.as_ref().map(|v| hash(v));
            let review: Option<Review> = bytes
                .map(|v| runtime.custody.open(&subject(&key), None, &v))
                .transpose()?;
            drop(trx);
            let envelope = review
                .as_ref()
                .map(|r| r.envelope(&target.source, &input_digest, credential, space))
                .transpose()?;
            let capsule = runtime
                .execution
                .read_native(&principal, Some(&fence))
                .await?;
            let root = capsule.root.clone();
            let core = Box::new(
                DeviceCore::restore(
                    self.open_image(&principal, capsule)?,
                    credential.unlocked()?,
                )
                .map_err(|_| RuntimeError::InvalidBinding)?,
            );
            let mut staging = Staging {
                runtime,
                custody: self,
                capability,
                credential,
                fence: &fence,
                key,
                prior,
                review,
                envelope,
                root,
                core,
                space,
                head: Seen {
                    seq: 0,
                    hash: String::new(),
                },
            };
            if let Some(progress) = staging.catch_up().await? {
                return Ok(progress);
            }
            if staging.review.is_none() {
                let (request, envelope, _) = staging
                    .core
                    .stage_image_task(space, source, input.clone())
                    .map_err(|_| RuntimeError::Denied)?;
                let wire = STANDARD.encode(
                    ClientFrame::Publish {
                        env: envelope.clone(),
                    }
                    .encode(),
                );
                staging.review = Some(Review {
                    source: target.source.clone(),
                    input: input_digest,
                    request: request.id,
                    content_hash: request
                        .tool_approval
                        .ok_or(RuntimeError::InvalidBinding)?
                        .content_hash,
                    wire,
                    accepted: None,
                    checkpoint: None,
                });
                staging.envelope = Some(envelope);
                staging.activate().await?;
            }
            staging.authorize().await?;
            let actor = event_log::Author::certified(
                credential.agent.id.clone(),
                event_log::Signer::from_secret(&credential.secret),
                credential.certificate.clone(),
            );
            let envelope = staging
                .envelope
                .as_ref()
                .ok_or(RuntimeError::InvalidBinding)?;
            let checkpoint = staging
                .review
                .as_ref()
                .ok_or(RuntimeError::InvalidBinding)?
                .checkpoint
                .as_ref();
            let mut transport = runtime
                .connect_bound_image_output(&target.url, &target.relay, actor, envelope, checkpoint)
                .await?;
            staging.authorize().await?;
            let envelope = staging
                .envelope
                .as_ref()
                .ok_or(RuntimeError::InvalidBinding)?;
            let checkpoint = staging
                .review
                .as_ref()
                .ok_or(RuntimeError::InvalidBinding)?
                .checkpoint
                .as_ref();
            let expected = runtime
                .image_publish_expectation(&target.relay, envelope, checkpoint)
                .await?;
            let seq = match expected {
                Some(expected) => transport.publish_managed(envelope, expected).await,
                None => transport.publish(envelope).await,
            }
            .map_err(super::delivery::transport_error)?;
            let position = runtime
                .execution
                .reply_source()?
                .position_in(&runtime.execution.transaction().await?, space, seq)
                .await
                .map_err(|_| RuntimeError::InvalidBinding)?;
            if &position.env != envelope {
                return Err(RuntimeError::InvalidBinding);
            }
            staging.catch_up().await?.ok_or(RuntimeError::Unavailable)
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
fn subject(key: &[u8]) -> String {
    format!("native-image-review/1/{}", hash(key))
}
impl Staging<'_> {
    async fn catch_up(&mut self) -> Result<Option<ImageReviewProgress>, RuntimeError> {
        let retained = self
            .core
            .retained_head(self.space)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let source = self.runtime.execution.reply_source()?;
        if let Some(accepted) = self.review.as_ref().and_then(|r| r.accepted.as_ref()) {
            let actual = source
                .position_in(
                    &self.runtime.execution.transaction().await?,
                    self.space,
                    accepted.seq,
                )
                .await
                .map_err(|_| RuntimeError::InvalidBinding)?;
            if Some(&actual.env) != self.envelope.as_ref() || actual.hash != accepted.hash {
                return Err(RuntimeError::InvalidBinding);
            }
            return Ok(Some(
                self.review
                    .as_ref()
                    .ok_or(RuntimeError::InvalidBinding)?
                    .progress()?,
            ));
        }
        let batch = source
            .read_in(
                &self.runtime.execution.transaction().await?,
                self.space,
                retained.as_ref().map(|h| (h.seq, h.hash.as_str())),
            )
            .await
            .map_err(|_| RuntimeError::Denied)?;
        self.head = Seen {
            seq: batch.head.seq(),
            hash: batch.head.hash().into(),
        };
        let mut accepted = None;
        for position in &batch.entries {
            if self
                .envelope
                .as_ref()
                .is_some_and(|env| env.client_id() == position.env.client_id())
            {
                if Some(&position.env) != self.envelope.as_ref() {
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
            self.review
                .as_mut()
                .ok_or(RuntimeError::InvalidBinding)?
                .accepted = Some(accepted);
            self.activate().await?;
            return Ok(Some(
                self.review
                    .as_ref()
                    .ok_or(RuntimeError::InvalidBinding)?
                    .progress()?,
            ));
        }
        if changed {
            self.activate().await?;
        }
        let through = self
            .core
            .retained_head(self.space)
            .map_err(|_| RuntimeError::InvalidBinding)?
            .ok_or(RuntimeError::InvalidBinding)?;
        if through != self.head {
            return Ok(Some(ImageReviewProgress::CatchingUp { through }));
        }
        Ok(None)
    }
    async fn authorize(&self) -> Result<(), RuntimeError> {
        let review = self.review.as_ref().ok_or(RuntimeError::InvalidBinding)?;
        self.core
            .image_review_ready(
                &review.request,
                &review.content_hash,
                self.envelope.as_ref().ok_or(RuntimeError::InvalidBinding)?,
                self.runtime.finance.reply_clock().await?,
            )
            .map_err(|_| RuntimeError::Denied)?;
        self.runtime
            .finance
            .native_directory(
                &self.credential.agent,
                &self.credential.owner,
                &self.credential.principal().device,
                &self.credential.certificate,
            )
            .await?
            .commit()
            .await?;
        drop(self.snapshot().await?);
        Ok(())
    }
}
