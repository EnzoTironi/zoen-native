//! An attachment's only durable content is the existing outbox plus local_chunks.
//! Only active producer sequences retain retry receipts. Restore never revives a lease.
use super::*;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

type UnpackedAttachment = (types::FileDoc, BTreeMap<String, Vec<u8>>);

const MAX_FILE_BYTES: usize = 64 << 20;
const MAX_HEADER_BYTES: usize = 256 << 10;
const MAX_FRAMED_BYTES: usize = MAX_FILE_BYTES + (2 << 20) + MAX_HEADER_BYTES;
const MAX_BATCH_FRAMED_BYTES: usize = MAX_FILE_BYTES + 10 * ((2 << 20) + MAX_HEADER_BYTES + 4);
pub(super) struct Producer {
    id: String,
    next: u64,
    // Present = issued, Some = accepted. Missing old sequences are retired forever
    // within this activation. The monotonically issued frontier prevents resurrection.
    pending: BTreeMap<u64, Option<Receipt>>,
}
impl Producer {
    pub(super) fn new() -> Self {
        Self {
            id: types::new_secret_id("attachment"),
            next: 1,
            pending: BTreeMap::new(),
        }
    }
    fn sequence(&self, mutation: &str) -> Result<u64, String> {
        let (producer, value) = mutation
            .rsplit_once('_')
            .ok_or("Invalid attachment producer.")?;
        let sequence = value
            .parse::<u64>()
            .map_err(|_| "Invalid attachment sequence.")?;
        if producer != self.id
            || sequence == 0
            || sequence >= self.next
            || sequence.to_string() != value
        {
            return Err("This attachment producer has retired. Reopen the conversation.".into());
        }
        Ok(sequence)
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum ProducerCommand {
    Begin {},
    Issue { producer_id: String },
    Accepted { mutation_id: String },
    Drop { mutation_id: String },
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Receipt {
    fingerprint: String,
    item: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    space_id: String,
    mutation_id: String,
    caption: String,
    reply_to: Option<String>,
    thread: bool,
    #[serde(default)]
    options: types::AttachmentSendOptions,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchRequest {
    space_id: String,
    reply_to: Option<String>,
    thread: bool,
    #[serde(default)]
    options: types::AttachmentSendOptions,
    items: Vec<BatchItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchItem {
    mutation_id: String,
    caption: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    doc: types::FileDoc,
    uploads: Vec<Upload>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Upload {
    blob: String,
    len: usize,
}

fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn valid_item(mutation: &str, caption: &str) -> bool {
    !mutation.is_empty()
        && mutation.len() <= 128
        && mutation
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        && caption.len() <= 16 << 10
        && !caption.contains('\0')
}

fn descriptor(doc: &types::FileDoc) -> serde_json::Value {
    serde_json::json!([
        doc.name,
        doc.mime,
        doc.bytes,
        doc.sha256,
        doc.thumb.as_ref().map(|thumb| (&thumb.sha256, thumb.bytes)),
    ])
}

/// The outer framing has exactly one bounded sealed frame for every ordered item.
fn unpack_batch(framed: &[u8], count: usize) -> Result<Vec<UnpackedAttachment>, String> {
    if framed.len() > MAX_BATCH_FRAMED_BYTES {
        return Err("Attachment batch too large.".into());
    }
    let mut offset = 0usize;
    let mut total = 0u64;
    let mut files = Vec::with_capacity(count);
    for _ in 0..count {
        if framed.len() - offset < 4 {
            return Err("Invalid attachment batch framing.".into());
        }
        let length = u32::from_le_bytes(framed[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        if length > framed.len() - offset {
            return Err("Invalid attachment batch framing.".into());
        }
        let file = unpack(&framed[offset..offset + length])?;
        total = total
            .checked_add(file.0.bytes)
            .ok_or("Attachment batch too large.")?;
        if total > MAX_FILE_BYTES as u64 {
            return Err("Attachment batch too large.".into());
        }
        files.push(file);
        offset += length;
    }
    if offset != framed.len() {
        return Err("Invalid attachment batch trailing bytes.".into());
    }
    Ok(files)
}

/// Validate the actual sealed frame, not caller-declared size/hash metadata alone.
fn unpack(framed: &[u8]) -> Result<UnpackedAttachment, String> {
    if framed.len() < 4 || framed.len() > MAX_FRAMED_BYTES {
        return Err("Invalid attachment payload.".into());
    }
    let length = u32::from_le_bytes(framed[..4].try_into().unwrap()) as usize;
    if length > MAX_HEADER_BYTES || length > framed.len() - 4 {
        return Err("Invalid attachment header.".into());
    }
    let Header { doc, uploads } =
        serde_json::from_slice(&framed[4..4 + length]).map_err(|_| "Invalid attachment header.")?;
    crate::media::validate_owned_file(&doc, MAX_FILE_BYTES)?;
    if !doc.path.is_empty()
        || doc.name.is_empty()
        || doc.name.len() > 255
        || doc.name.contains(['/', '\\', '\0'])
        || !doc.mime.contains('/')
        || doc.mime.len() > 128
        || doc.mime.chars().any(char::is_control)
        || doc.chunks.len() > 65
        || uploads.len() > 66
    {
        return Err("Invalid attachment metadata.".into());
    }
    let wanted = doc
        .chunks
        .iter()
        .chain(doc.thumb.iter())
        .map(|chunk| chunk.blob.as_str())
        .collect::<BTreeSet<_>>();
    if uploads.len() != wanted.len() {
        return Err("Invalid attachment chunks.".into());
    }
    let mut chunks = BTreeMap::new();
    let mut offset = 4 + length;
    for upload in uploads {
        if !wanted.contains(upload.blob.as_str())
            || chunks.contains_key(&upload.blob)
            || upload.len > framed.len() - offset
        {
            return Err("Invalid attachment chunks.".into());
        }
        chunks.insert(upload.blob, framed[offset..offset + upload.len].to_vec());
        offset += upload.len;
    }
    if offset != framed.len() {
        return Err("Invalid attachment trailing bytes.".into());
    }
    let mut digest = Sha256::new();
    let mut actual_bytes = 0usize;
    for chunk in &doc.chunks {
        let plain = Zeroizing::new(crate::media::open_chunk(chunk, &chunks[&chunk.blob])?);
        if plain.len() as u64 != chunk.bytes || plain.len() > MAX_FILE_BYTES - actual_bytes {
            return Err("Attachment chunk length does not match its manifest.".into());
        }
        actual_bytes += plain.len();
        digest.update(plain.as_slice());
    }
    if actual_bytes as u64 != doc.bytes || hex::encode(digest.finalize()) != doc.sha256 {
        return Err("Attachment content does not match its manifest.".into());
    }
    if let Some(thumb) = &doc.thumb {
        let plain = Zeroizing::new(crate::media::open_chunk(thumb, &chunks[&thumb.blob])?);
        if plain.len() as u64 != thumb.bytes {
            return Err("Attachment thumbnail length does not match its manifest.".into());
        }
    }
    Ok((doc, chunks))
}

impl<S: Signer> Client<S> {
    /// Ephemeral admission only. No producer token or sequence can be restored from
    /// caller input, saved snapshots, or an old activation's pending callback.
    pub fn attachment_producer(&mut self, request: &str) -> Result<String, String> {
        if request.len() > 1024 {
            return Err("Invalid attachment producer request.".into());
        }
        let command: ProducerCommand =
            serde_json::from_str(request).map_err(|_| "Invalid attachment producer request.")?;
        match command {
            ProducerCommand::Begin {} => Ok(self.attachment_producer.id.clone()),
            ProducerCommand::Issue { producer_id } => {
                if producer_id != self.attachment_producer.id {
                    return Err("This attachment producer has retired.".into());
                }
                let sequence = self.attachment_producer.next;
                self.attachment_producer.next = sequence
                    .checked_add(1)
                    .ok_or("Attachment sequence exhausted.")?;
                self.attachment_producer.pending.insert(sequence, None);
                Ok(format!("{}_{sequence}", self.attachment_producer.id))
            }
            ProducerCommand::Accepted { mutation_id } => {
                let sequence = self.attachment_producer.sequence(&mutation_id)?;
                if matches!(self.attachment_producer.pending.get(&sequence), Some(None)) {
                    return Err("The UI cannot acknowledge an unaccepted attachment.".into());
                }
                self.attachment_producer.pending.remove(&sequence);
                Ok(String::new())
            }
            ProducerCommand::Drop { mutation_id } => {
                let sequence = self.attachment_producer.sequence(&mutation_id)?;
                self.attachment_producer.pending.remove(&sequence);
                Ok(String::new())
            }
        }
    }

    /// Called only after explicit Send. Both signed events and all ciphertext are
    /// retained before the host can upload or publish anything. A fresh encryption
    /// of the same retry is allowed, but cannot replace the accepted ciphertext.
    pub fn queue_attachment(
        &mut self,
        request: &str,
        framed: &[u8],
        now: i64,
    ) -> Result<(Update, String, String), String> {
        if request.len() > 128 << 10 {
            return Err("Attachment request too large.".into());
        }
        let request: Request =
            serde_json::from_str(request).map_err(|_| "Invalid attachment request.")?;
        if !identifier(&request.space_id)
            || !valid_item(&request.mutation_id, &request.caption)
            || request.reply_to.as_ref().is_some_and(|id| !identifier(id))
            || request.thread && request.reply_to.is_none()
            || !request.options.validate_count(1)
        {
            return Err("Invalid attachment intent.".into());
        }
        let sequence = self.attachment_producer.sequence(&request.mutation_id)?;
        let receipt = self
            .attachment_producer
            .pending
            .get(&sequence)
            .ok_or("This attachment sequence has retired.")?
            .clone();
        self.document_writer(&request.space_id)?;
        let (doc, chunks) = unpack(framed)?;
        if !request.options.is_default() && doc.is_voice() {
            return Err("Voice notes cannot use attachment presentation options.".into());
        }
        // Encryption keys/nonces are deliberately excluded. The checked plaintext
        // descriptor and exact unnormalised target/caption identify this intent.
        let legacy = serde_json::json!([
            request.space_id,
            request.caption,
            request.reply_to,
            request.thread,
            doc.name,
            doc.mime,
            doc.bytes,
            doc.sha256,
            doc.thumb.as_ref().map(|thumb| (&thumb.sha256, thumb.bytes)),
        ]);
        // The old no-options fingerprint is byte-for-byte unchanged.
        let intent = if request.options.is_default() {
            legacy
        } else {
            serde_json::json!(["zoen.attachment.intent.v2", legacy, request.options])
        };
        let fingerprint = hex::encode(Sha256::digest(
            serde_json::to_vec(&intent).map_err(|error| error.to_string())?,
        ));
        if let Some(receipt) = receipt {
            if receipt.fingerprint != fingerprint {
                return Err(
                    "This attachment retry has different content or a different target.".into(),
                );
            }
            return Ok((self.update(Vec::new())?, receipt.item, receipt.message));
        }
        let reply = match &request.reply_to {
            Some(to) => {
                let target = self
                    .posted(&request.space_id, to)
                    .ok_or("That message is no longer available.")?;
                let reference = match target {
                    Some(root) if request.thread && root.thread => root.to,
                    _ => to.clone(),
                };
                let event = self
                    .resolve_post(&request.space_id, &reference)
                    .ok_or("That message is no longer available.")?;
                Some(types::ReplyRef {
                    to: event.hash.clone(),
                    thread: request.thread,
                })
            }
            None => None,
        };
        let item = types::new_secret_id("it");
        let uploads = chunks.keys().cloned().collect::<Vec<_>>();
        let identities = self.queue_domain_batch_uploads(
            &request.space_id,
            vec![
                EventBody::ItemCreated {
                    item: item.clone(),
                    kind: types::ItemKind::File,
                    content: types::ItemContent::File(doc),
                    origin: "Adicionado".into(),
                },
                EventBody::MessagePosted {
                    message: format!("msg_attachment_{item}"),
                    text: request.caption,
                    attaches: Some(item.clone()),
                    reply,
                    forwarded: false,
                    media: request.options.metadata(
                        &self.account.identity,
                        &request.space_id,
                        request.reply_to.as_deref(),
                        request.thread,
                        &[request.mutation_id.as_str()],
                        0,
                    ),
                },
            ],
            now,
            &uploads,
        )?;
        let message = identities[1].clone();
        self.saved.local_chunks.extend(chunks);
        self.attachment_producer.pending.insert(
            sequence,
            Some(Receipt {
                fingerprint,
                item: item.clone(),
                message: message.clone(),
            }),
        );
        Ok((self.update(Vec::new())?, item, message))
    }

    /// One immutable intent, one atomic outbox batch, and one shared upload barrier.
    /// No event or retained ciphertext changes until all entries have been validated.
    pub fn queue_attachment_batch(
        &mut self,
        request: &str,
        framed: &[u8],
        now: i64,
    ) -> Result<(Update, Vec<(String, String)>), String> {
        if request.len() > 256 << 10 {
            return Err("Attachment batch request too large.".into());
        }
        let request: BatchRequest =
            serde_json::from_str(request).map_err(|_| "Invalid attachment batch request.")?;
        if !identifier(&request.space_id)
            || request.reply_to.as_ref().is_some_and(|id| !identifier(id))
            || request.thread && request.reply_to.is_none()
            || !request.options.validate_count(request.items.len())
            || request
                .items
                .iter()
                .any(|item| !valid_item(&item.mutation_id, &item.caption))
            || request
                .items
                .iter()
                .map(|item| &item.mutation_id)
                .collect::<BTreeSet<_>>()
                .len()
                != request.items.len()
        {
            return Err("Invalid attachment batch intent.".into());
        }
        self.document_writer(&request.space_id)?;
        let issued = request
            .items
            .iter()
            .map(|item| {
                let sequence = self.attachment_producer.sequence(&item.mutation_id)?;
                let receipt = self
                    .attachment_producer
                    .pending
                    .get(&sequence)
                    .ok_or("This attachment sequence has retired.")?
                    .clone();
                Ok((sequence, receipt))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let files = unpack_batch(framed, request.items.len())?;
        if !request.options.is_default() && files.iter().any(|(doc, _)| doc.is_voice()) {
            return Err("Voice notes cannot use attachment presentation options.".into());
        }
        let intent = serde_json::json!([
            "zoen.attachment.batch.v1",
            self.account.identity,
            request.space_id,
            request.reply_to,
            request.thread,
            request.options,
            request
                .items
                .iter()
                .zip(&files)
                .map(|(item, (doc, _))| {
                    serde_json::json!([item.mutation_id, item.caption, descriptor(doc)])
                })
                .collect::<Vec<_>>(),
        ]);
        let fingerprint = hex::encode(Sha256::digest(
            serde_json::to_vec(&intent).map_err(|error| error.to_string())?,
        ));
        if issued.iter().any(|(_, receipt)| receipt.is_some()) {
            let replay = issued.iter().map(|(_, receipt)| {
                let receipt = receipt.as_ref().ok_or("This attachment batch was only partly accepted.")?;
                if receipt.fingerprint != fingerprint {
                    return Err("This attachment batch retry has different content or a different target.".into());
                }
                Ok((receipt.item.clone(), receipt.message.clone()))
            }).collect::<Result<Vec<_>, String>>()?;
            return Ok((self.update(Vec::new())?, replay));
        }
        let reply = match &request.reply_to {
            Some(to) => {
                let target = self
                    .posted(&request.space_id, to)
                    .ok_or("That message is no longer available.")?;
                let reference = match target {
                    Some(root) if request.thread && root.thread => root.to,
                    _ => to.clone(),
                };
                let event = self
                    .resolve_post(&request.space_id, &reference)
                    .ok_or("That message is no longer available.")?;
                Some(types::ReplyRef {
                    to: event.hash.clone(),
                    thread: request.thread,
                })
            }
            None => None,
        };
        let ordered = request
            .items
            .iter()
            .map(|item| item.mutation_id.as_str())
            .collect::<Vec<_>>();
        let mut bodies = Vec::with_capacity(request.items.len() * 2);
        let mut chunks = BTreeMap::new();
        let mut items = Vec::with_capacity(request.items.len());
        for (index, (intent, (doc, file_chunks))) in request.items.iter().zip(files).enumerate() {
            for (blob, bytes) in file_chunks {
                if chunks.get(&blob).is_some_and(|old| old != &bytes) {
                    return Err("Conflicting attachment ciphertext.".into());
                }
                chunks.insert(blob, bytes);
            }
            let item = types::new_secret_id("it");
            bodies.push(EventBody::ItemCreated {
                item: item.clone(),
                kind: types::ItemKind::File,
                content: types::ItemContent::File(doc),
                origin: "Adicionado".into(),
            });
            bodies.push(EventBody::MessagePosted {
                message: format!("msg_attachment_{item}"),
                text: intent.caption.clone(),
                attaches: Some(item.clone()),
                reply: reply.clone(),
                forwarded: false,
                media: request.options.metadata(
                    &self.account.identity,
                    &request.space_id,
                    request.reply_to.as_deref(),
                    request.thread,
                    &ordered,
                    index,
                ),
            });
            items.push(item);
        }
        let uploads = chunks.keys().cloned().collect::<Vec<_>>();
        let identities =
            self.queue_domain_batch_uploads(&request.space_id, bodies, now, &uploads)?;
        let mut result = Vec::with_capacity(items.len());
        for (index, (item, (sequence, _))) in items.into_iter().zip(issued).enumerate() {
            let message = identities[index * 2 + 1].clone();
            self.attachment_producer.pending.insert(
                sequence,
                Some(Receipt {
                    fingerprint: fingerprint.clone(),
                    item: item.clone(),
                    message: message.clone(),
                }),
            );
            result.push((item, message));
        }
        self.saved.local_chunks.extend(chunks);
        Ok((self.update(Vec::new())?, result))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
