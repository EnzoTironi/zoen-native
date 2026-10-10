//! Stable discovery records. These are inert retained originals; the loader
//! derives current native facts separately and does not create a spend permit.
mod cancellation;
mod storage;
pub(crate) use cancellation::{CleanupAcquisition, CleanupStep};
#[cfg(test)]
pub(super) mod testing;

use roda_ffi::runtime_core::{NativeError, ReplyFacts, ReplyIntent};
use roda_types::Seen;
use zoen_models::{InputMessage, Operation};

use super::*;
use crate::{
    execution::{Execution, Fence},
    Binding, ModelRequest, ReplyRun, ReplySync,
};

const PROFILE: &str = "owner-direct-text-reply/1";
const MAX_RUNS: usize = 16;
const MAX_BATCH_BYTES: usize = 2 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunRecord {
    format: u8,
    source: String,
    locator: ReplyRun,
    request: ModelRequest,
    binding: Binding,
    intent: ReplyIntent,
    original: Root,
    head: Seen,
    epoch: u64,
}

impl RunRecord {
    fn validate(&self, run: &str, source: &str) -> Result<(), RuntimeError> {
        let context = &self.request.context;
        let principal = &self.original.context.principal;
        self.original
            .context
            .validate(principal, self.original.context.key_version)?;
        if self.format != 1
            || self.source != source
            || self.locator.run != run
            || ![&self.locator.run, &self.locator.attempt, &self.locator.wake]
                .into_iter()
                .all(|value| id(value))
            || context != &self.binding.context
            || context.run_id != run
            || context.attempt_id != self.locator.attempt
            || context.owner != principal.owner
            || context.agent != principal.agent
            || context.device != principal.device
            || hash(self.binding.device_cert.as_bytes()) != principal.certificate_digest
            || context.space != self.intent.space()
            || context.agent != self.intent.agent()
            || !digest(self.intent.trigger())
            || !digest(&self.head.hash)
            || !digest(&self.original.manifest_digest)
            || !digest(&self.binding.request_digest)
            || context.authority_version != self.head.hash
            || self.binding.frontier != self.head.hash
            || context.definition_version != PROFILE
            || context.price_version != self.binding.price.version
            || self.request.operation != Operation::ChatCompletion
            || self.request.max_output_tokens != self.binding.requested_output_tokens
            || !self.request.tools.is_empty()
            || !matches!(
                self.request.messages.as_slice(),
                [InputMessage::User { .. }]
            )
            || self.binding.hold_units
                != self.binding.price.reserve(self.request.max_output_tokens)?
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Wake {
    locator: ReplyRun,
    record_digest: String,
}

#[derive(Serialize)]
struct PreparedRun {
    trigger: String,
    locator: ReplyRun,
    record: Vec<u8>,
    binding: Vec<u8>,
    wake: Vec<u8>,
}

#[derive(Serialize)]
struct Batch {
    source: String,
    space: String,
    head: Seen,
    runs: Vec<PreparedRun>,
    #[cfg(test)]
    #[serde(skip)]
    cut: u8,
}

impl Batch {
    fn scope(
        &self,
        execution: &Execution,
        staged: &super::storage::Staged,
        fence: &Fence,
    ) -> Result<ActivationScope, RuntimeError> {
        if self.runs.len() > MAX_RUNS {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let bytes = serde_json::to_vec(self).map_err(|_| RuntimeError::InvalidBinding)?;
        if bytes.len() > MAX_BATCH_BYTES {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let mut scope = staged.scope(execution, fence);
        scope.effects = hash(&bytes);
        Ok(scope)
    }
}

struct LoadedReply {
    record: RunRecord,
    record_digest: String,
    current: Root,
    facts: ReplyFacts,
    fences: ReplyFences,
}

/// Owns one actually loaded original and separately refreshed authorization.
/// Only the private native loader can construct this non-replayable step.
pub(crate) struct ReplyStep {
    loaded: LoadedReply,
}

impl ReplyStep {
    pub(crate) fn request(&self) -> &ModelRequest {
        &self.loaded.record.request
    }

    pub(crate) fn binding(
        &self,
        request: &zoen_models::DispatchRequest,
    ) -> Result<Binding, RuntimeError> {
        let original = &self.loaded.record.binding;
        if request.context != original.context
            || request.request_digest != original.request_digest
            || request.descriptor.requested_output_tokens != original.requested_output_tokens
            || original.price.quote(request)? != original.hold_units
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(original.clone())
    }

    pub(crate) fn grant_deadline(&self) -> Option<i64> {
        self.loaded.facts.expires_at_ms()
    }

    pub(crate) async fn scope_in(
        &self,
        execution: &Execution,
        trx: &foundationdb::Transaction,
        custody: &crate::custody::Custody,
    ) -> Result<(), RuntimeError> {
        execution
            .verify_reply_in(
                trx,
                &self.loaded.record,
                &self.loaded.record_digest,
                &self.loaded.current,
                &self.loaded.facts,
                &self.loaded.fences,
                custody,
            )
            .await
    }

    pub(crate) async fn release(&self, execution: &Execution) -> Result<(), RuntimeError> {
        execution
            .release_reply(&self.loaded.record, &self.loaded.fences)
            .await
    }
}

// Constructed by known atomic acquisition of a real retained run and device.
// Neither fence is optional, deserialized or accepted from a public caller.
struct ReplyFences {
    run: Fence,
    device: Fence,
}

fn native_error(error: NativeError) -> RuntimeError {
    match error {
        NativeError::Bounds | NativeError::Image => RuntimeError::InvalidBinding,
        NativeError::Storage => RuntimeError::Unavailable,
        _ => RuntimeError::Denied,
    }
}

impl NativeCustody {
    pub(crate) async fn sync_replies(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
        space: &str,
    ) -> Result<ReplySync, RuntimeError> {
        let credential = self.credential(agent, device)?;
        let principal = credential.principal();
        runtime.execution.reply_source()?;
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
        let result = self
            .sync_fenced(runtime, credential, &principal, &fence, space)
            .await;
        let released = runtime
            .execution
            .release_native_device(&principal, &fence)
            .await;
        match result {
            Err(error) => Err(error),
            Ok(report) => {
                released?;
                Ok(report)
            }
        }
    }

    async fn sync_fenced(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        principal: &Principal,
        fence: &Fence,
        space: &str,
    ) -> Result<ReplySync, RuntimeError> {
        let capsule = runtime
            .execution
            .read_native(principal, Some(fence))
            .await?;
        let previous = capsule.root.clone();
        let mut core =
            DeviceCore::restore(self.open_image(principal, capsule)?, credential.unlocked()?)
                .map_err(native_error)?;
        let retained = core.retained_head(space).map_err(native_error)?;
        let source = runtime.execution.reply_source()?;
        let trx = runtime.execution.transaction().await?;
        let batch = source
            .read_in(
                &trx,
                space,
                retained.as_ref().map(|head| (head.seq, head.hash.as_str())),
            )
            .await
            .map_err(|_| RuntimeError::Denied)?;
        drop(trx);
        let applied = !batch.entries.is_empty();
        core.ingest(batch.entries).map_err(native_error)?;
        let head = Seen {
            seq: batch.head.seq(),
            hash: batch.head.hash().into(),
        };
        let caught_up = core.retained_head(space).map_err(native_error)?.as_ref() == Some(&head);
        let discovery = if caught_up {
            Some(
                core.discover_replies(space, runtime.finance.reply_clock().await?)
                    .map_err(native_error)?,
            )
        } else {
            None
        };
        let scanned = discovery.as_ref().map(|scan| scan.examined).unwrap_or(0);
        let discovery_complete = discovery.as_ref().is_some_and(|scan| scan.complete);
        let changed = applied || discovery.as_ref().is_some_and(|scan| scan.changed);
        if !changed {
            runtime
                .finance
                .native_directory(
                    &credential.agent,
                    &credential.owner,
                    &principal.device,
                    &credential.certificate,
                )
                .await?
                .commit()
                .await?;
            return Ok(ReplySync {
                native: NativeInspection {
                    generation: previous.context.generation,
                    image_bytes: previous.context.image_bytes,
                },
                caught_up,
                scanned,
                discovery_complete,
                runs: Vec::new(),
            });
        }
        let next = self.seal_image(
            principal,
            Some(&previous),
            &core.image().map_err(native_error)?,
        )?;
        let target = next.root.clone();
        drop(core);
        let mut effects = Batch {
            source: source.cell().into(),
            space: space.into(),
            head,
            runs: Vec::new(),
            #[cfg(test)]
            cut: runtime.fault.load(std::sync::atomic::Ordering::SeqCst),
        };
        if let Some(discovery) = discovery {
            if !discovery.replies.is_empty() {
                let (period_start, policy_digest) = runtime
                    .finance
                    .reply_budget(&principal.owner, &runtime.price)
                    .await?;
                for facts in discovery.replies {
                    if facts.frontier().seq() != effects.head.seq
                        || facts.frontier().hash() != effects.head.hash
                    {
                        return Err(RuntimeError::Denied);
                    }
                    let locator = ReplyRun {
                        run: roda_types::new_id("run"),
                        attempt: roda_types::new_id("attempt"),
                        wake: roda_types::new_id("wake"),
                    };
                    let request = ModelRequest {
                        context: zoen_models::AttemptContext {
                            run_id: locator.run.clone(),
                            attempt_id: locator.attempt.clone(),
                            owner: principal.owner.clone(),
                            agent: principal.agent.clone(),
                            device: principal.device.clone(),
                            space: space.into(),
                            authority_version: effects.head.hash.clone(),
                            definition_version: PROFILE.into(),
                            price_version: runtime.price.version.clone(),
                        },
                        operation: Operation::ChatCompletion,
                        messages: vec![InputMessage::User {
                            text: facts.text().into(),
                        }],
                        tools: Vec::new(),
                        max_output_tokens: runtime.price.max_output_tokens,
                    };
                    let dispatch = runtime
                        .gateway
                        .preflight(&request)
                        .map_err(|_| RuntimeError::InvalidProfile)?;
                    let binding = Binding {
                        context: request.context.clone(),
                        device_cert: credential.certificate.clone(),
                        frontier: effects.head.hash.clone(),
                        period_start,
                        policy_digest: policy_digest.clone(),
                        request_digest: dispatch.request_digest.clone(),
                        requested_output_tokens: request.max_output_tokens,
                        hold_units: runtime.price.quote(&dispatch)?,
                        price: runtime.price.clone(),
                    };
                    let record = RunRecord {
                        format: 1,
                        source: effects.source.clone(),
                        locator: locator.clone(),
                        request,
                        binding,
                        intent: facts.intent().clone(),
                        original: target.clone(),
                        head: effects.head.clone(),
                        epoch: facts.frontier().epoch(),
                    };
                    record.validate(&locator.run, source.cell())?;
                    let (_, sealed) = runtime
                        .custody
                        .seal(&format!("native-run/1/{}", locator.run), &record)?;
                    let (_, frozen) = runtime
                        .custody
                        .seal(&format!("prepared/{}", locator.attempt), &record.binding)?;
                    let (_, wake) = runtime.custody.seal(
                        &format!("native-wake/1/{}", locator.wake),
                        &Wake {
                            locator: locator.clone(),
                            record_digest: hash(&sealed),
                        },
                    )?;
                    effects.runs.push(PreparedRun {
                        trigger: record.intent.trigger().into(),
                        locator,
                        record: sealed,
                        binding: frozen,
                        wake,
                    });
                }
            }
        }
        let staged = runtime
            .execution
            .stage_native(principal, Some(previous), fence, next)
            .await?;
        let scope = effects.scope(&runtime.execution, &staged, fence)?;
        #[cfg(test)]
        let scope = if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 18 {
            staged.scope(&runtime.execution, fence)
        } else {
            scope
        };
        let admission = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?
            .authorize(scope)
            .await?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 19 {
            effects.runs[0].record[0] ^= 1;
        }
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 20 {
            runtime.native_cut_entered.notify_one();
            runtime.native_cut_resume.notified().await;
        }
        let (root, runs) = runtime
            .execution
            .activate_replies(staged, fence, admission, effects, &runtime.custody)
            .await?;
        Ok(ReplySync {
            native: NativeInspection {
                generation: root.context.generation,
                image_bytes: root.context.image_bytes,
            },
            caught_up,
            scanned,
            discovery_complete,
            runs,
        })
    }

    async fn load_reply(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
    ) -> Result<LoadedReply, RuntimeError> {
        let (record, record_digest) = runtime.execution.read_reply(run, &runtime.custody).await?;
        let context = &record.request.context;
        let credential = self.credential(&context.agent, &context.device)?;
        let principal = credential.principal();
        if principal != record.original.context.principal {
            return Err(RuntimeError::InvalidBinding);
        }
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        let fences = runtime
            .execution
            .acquire_reply(&record, &record_digest)
            .await?;
        let result = async {
            let capsule = runtime
                .execution
                .read_native(&principal, Some(&fences.device))
                .await?;
            let current = capsule.root.clone();
            let observed = runtime.finance.reply_clock().await?;
            let facts = {
                let core = DeviceCore::restore(
                    self.open_image(&principal, capsule)?,
                    credential.unlocked()?,
                )
                .map_err(native_error)?;
                core.refresh(&record.intent, observed)
                    .map_err(native_error)?
            };
            let [InputMessage::User { text }] = record.request.messages.as_slice() else {
                return Err(RuntimeError::InvalidBinding);
            };
            if text != facts.text() {
                return Err(RuntimeError::InvalidBinding);
            }
            let dispatch = runtime
                .gateway
                .preflight(&record.request)
                .map_err(|_| RuntimeError::InvalidProfile)?;
            if dispatch.request_digest != record.binding.request_digest
                || record.binding.price.quote(&dispatch)? != record.binding.hold_units
            {
                return Err(RuntimeError::InvalidBinding);
            }
            runtime
                .execution
                .verify_reply(
                    &record,
                    &record_digest,
                    &current,
                    &facts,
                    &fences,
                    &runtime.custody,
                )
                .await?;
            runtime
                .finance
                .native_directory(
                    &credential.agent,
                    &credential.owner,
                    &principal.device,
                    &credential.certificate,
                )
                .await?
                .commit()
                .await?;
            Ok((facts, current))
        }
        .await;
        match result {
            Ok((facts, current)) => Ok(LoadedReply {
                record,
                record_digest,
                current,
                facts,
                fences,
            }),
            Err(error) => {
                let _ = runtime.execution.release_reply(&record, &fences).await;
                Err(error)
            }
        }
    }

    pub(crate) async fn verified_reply(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
    ) -> Result<ReplyStep, RuntimeError> {
        Ok(ReplyStep {
            loaded: self.load_reply(runtime, run).await?,
        })
    }

    pub(crate) async fn inspect_reply(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
    ) -> Result<ReplyRun, RuntimeError> {
        let loaded = self.load_reply(runtime, run).await?;
        // Derived current authorization is deliberately separate from the frozen
        // original binding. No VerifiedStep or dispatch permit is constructed.
        if loaded.facts.intent() != &loaded.record.intent {
            return Err(RuntimeError::InvalidBinding);
        }
        runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await?;
        Ok(loaded.record.locator)
    }
}
