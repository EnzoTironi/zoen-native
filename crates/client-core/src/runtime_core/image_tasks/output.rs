use super::*;
use protocol::Envelope;
use sha2::{Digest, Sha256};

pub struct PreparedImageTask {
    facts: ImageTaskFacts,
    file: FileDoc,
    envelope: Envelope,
    image: NativeImage,
    blobs: Vec<crate::files::NativeFileBlob>,
}
impl std::fmt::Debug for PreparedImageTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedImageTask { redacted }")
    }
}
impl PreparedImageTask {
    pub fn blobs(&self) -> &[crate::files::NativeFileBlob] {
        &self.blobs
    }
    pub fn facts(&self) -> &ImageTaskFacts {
        &self.facts
    }
    pub fn file(&self) -> &FileDoc {
        &self.file
    }
    pub fn envelope(&self) -> &Envelope {
        &self.envelope
    }
    pub fn image(&self) -> &NativeImage {
        &self.image
    }
}
fn validate_png(bytes: &[u8], definition: &crate::image_tasks::ImageTaskDefinition) -> Result<()> {
    if bytes.is_empty()
        || bytes.len() as u64 > definition.max_image_bytes
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
    {
        return Err(NativeError::Bounds);
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.ignore_checksums(false);
    decoder.set_limits(png::Limits {
        bytes: 16 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().map_err(|_| NativeError::Bounds)?;
    let dimensions = match definition.size.as_str() {
        "1024x1024" => (1024, 1024),
        "1536x1024" => (1536, 1024),
        "1024x1536" => (1024, 1536),
        _ => return Err(NativeError::Bounds),
    };
    if (reader.info().width, reader.info().height) != dimensions
        || reader.info().animation_control.is_some()
    {
        return Err(NativeError::Bounds);
    }
    let size = reader
        .output_buffer_size()
        .filter(|size| *size <= 16 * 1024 * 1024)
        .ok_or(NativeError::Bounds)?;
    reader
        .next_frame(&mut vec![0; size])
        .map_err(|_| NativeError::Bounds)?;
    reader.finish().map_err(|_| NativeError::Bounds)
}
impl DeviceCore {
    /// Converts verified bytes into the existing encrypted FileDoc/Image path.
    /// The caller must fence and activate the whole returned native image before
    /// publishing or uploading anything. Preparation is not transport authority.
    pub fn prepare_image_task(
        &mut self,
        request: &str,
        observed_hash: &str,
        bytes: &[u8],
        at_ms: i64,
    ) -> Result<PreparedImageTask> {
        self.prepare_image_task_storage(request, observed_hash, bytes, at_ms, false)
    }

    /// The capsule retains only canonical FileDoc/key/upload references. The
    /// caller must retain every returned ciphertext before fenced activation.
    pub fn prepare_image_task_referenced(
        &mut self,
        request: &str,
        observed_hash: &str,
        bytes: &[u8],
        at_ms: i64,
    ) -> Result<PreparedImageTask> {
        self.prepare_image_task_storage(request, observed_hash, bytes, at_ms, true)
    }

    fn prepare_image_task_storage(
        &mut self,
        request: &str,
        observed_hash: &str,
        bytes: &[u8],
        at_ms: i64,
        external: bool,
    ) -> Result<PreparedImageTask> {
        let result = (|| {
            let facts = self.image_task(request, at_ms)?;
            if facts.content_hash() != observed_hash
                || self
                    .engine
                    .store
                    .outbox_len()
                    .map_err(|_| NativeError::Storage)?
                    != 0
            {
                return Err(NativeError::Permission);
            }
            validate_png(bytes, &facts.input.definition)?;
            let item = facts.proposal.artifact.clone();
            let space = facts.space.clone();
            let actor = facts.agent.clone();
            let name = format!("{item}.png");
            let (file, event, blobs) = self
                .engine
                .atomic_agent_action(|engine| {
                    let key = engine.file_key(None)?;
                    let (file, blobs) = if external {
                        engine.native_file_doc(&space, &key, &name, bytes)?
                    } else {
                        (
                            engine.file_doc(
                                &space,
                                &key,
                                &name,
                                &name,
                                "image/png",
                                bytes,
                                None,
                                &std::collections::HashMap::new(),
                            )?,
                            Vec::new(),
                        )
                    };
                    engine
                        .store
                        .set_meta(&format!("file_key:{item}"), &hex::encode(key))?;
                    let event = engine.append(
                        &space,
                        &actor,
                        EventBody::ItemCreated {
                            item: item.clone(),
                            kind: ItemKind::File,
                            content: ItemContent::File(file.clone()),
                            origin: String::new(),
                        },
                    )?;
                    engine.store.set_meta(
                        &format!("runtime.image-task.result/1/{request}"),
                        &serde_json::to_string(&(observed_hash, &item, &file.sha256)).map_err(
                            |_| crate::CoreError::Invalid {
                                reason: "Invalid image receipt.".into(),
                            },
                        )?,
                    )?;
                    Ok((file, event, blobs))
                })
                .map_err(|_| NativeError::Storage)?;
            self.engine
                .mls_seal_outbox()
                .map_err(|_| NativeError::Storage)?;
            // Prepare and retain the producer's original, without bypassing the
            // ordinary upload-dependent outbox offer gate.
            let envelope = self
                .engine
                .outgoing_envelope_with(&event, &mut std::collections::HashMap::new())
                .ok_or(NativeError::NotReady)?;
            if !envelope.is_sealed()
                || envelope.is_pruned()
                || envelope.verify().is_err()
                || envelope.space() != space
                || envelope.author() != actor
                || envelope.content().len() > MAX_EVENT_BYTES
                || file.sha256 != hex::encode(Sha256::digest(bytes))
            {
                return Err(NativeError::History);
            }
            self.retain_image_task_reference(request, &event, &envelope)?;
            Ok(PreparedImageTask {
                facts,
                blobs,
                file,
                envelope: envelope.clone(),
                image: self.image()?,
            })
        })();
        if result.is_err() {
            self.usable = false;
        }
        result
    }
}
