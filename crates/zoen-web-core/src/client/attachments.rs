//! An attachment's only durable content is the existing outbox plus local_chunks.
//! Only active producer sequences retain retry receipts. Restore never revives a lease.
use super::*;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const MAX_FILE_BYTES: usize = 64 << 20;
const MAX_HEADER_BYTES: usize = 256 << 10;
const MAX_FRAMED_BYTES: usize = MAX_FILE_BYTES + (2 << 20) + MAX_HEADER_BYTES;
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

/// Validate the actual sealed frame, not caller-declared size/hash metadata alone.
fn unpack(framed: &[u8]) -> Result<(types::FileDoc, BTreeMap<String, Vec<u8>>), String> {
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
            || request.mutation_id.is_empty()
            || request.mutation_id.len() > 128
            || !request
                .mutation_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            || request.caption.len() > 16 << 10
            || request.caption.contains('\0')
            || request.reply_to.as_ref().is_some_and(|id| !identifier(id))
            || request.thread && request.reply_to.is_none()
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
        // Encryption keys/nonces are deliberately excluded. The checked plaintext
        // descriptor and exact unnormalised target/caption identify this intent.
        let fingerprint = hex::encode(Sha256::digest(
            serde_json::to_vec(&serde_json::json!([
                request.space_id,
                request.caption,
                request.reply_to,
                request.thread,
                doc.name,
                doc.mime,
                doc.bytes,
                doc.sha256,
                doc.thumb.as_ref().map(|thumb| (&thumb.sha256, thumb.bytes)),
            ]))
            .map_err(|error| error.to_string())?,
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
}
