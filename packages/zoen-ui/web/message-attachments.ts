// Whole operations borrowed from the sole Messages owner.
import {
  type VoiceMeta,
  type AttachmentIntent,
  attachmentOptions,
  parseFile,
  parseVoice,
  type AttachmentBatchFile,
  type AttachmentBatchIntent,
  type FileDoc,
  MAX_FILE_BYTES,
  MAX_THUMBNAIL_BYTES,
} from './message-file-model.ts';
import { object, array, string, integer, message } from './boundary.ts';
import { plain } from './message-model.ts';
import type { MessageOperations } from './message-operations.ts';
type MessageAttachmentsPort = Pick<
  MessageOperations,
  | 'core'
  | 'sendAttachment'
  | 'upload'
  | 'queueFileRecord'
  | 'closed'
  | 'conversations'
  | 'generation'
  | 'enqueue'
  | 'apply'
  | 'sendAttachmentRequest'
  | 'attachmentProducer'
  | 'connection'
  | 'resumeDocumentUploads'
  | 'notice'
  | 'scheduleMaintenance'
  | 'publish'
  | 'documentRequest'
  | 'act'
>;

/** Seal like native, upload every encrypted chunk, then post the file into the chat. */
export async function sendFile(
  this: MessageAttachmentsPort,
  space: string,
  file: {
    name: string;
    mime: string;
    bytes: Uint8Array;
    thumb?: Uint8Array | null;
    voice?: VoiceMeta;
    attachment?: AttachmentIntent;
  },
  progress?: (done: number, total: number) => void,
): Promise<{ itemId: string; messageId: string }> {
  const options = file.attachment ? attachmentOptions(file.attachment.options, 1) : undefined;
  const attachment = file.attachment
    ? Object.freeze({
        mutationId: file.attachment.mutationId,
        caption: file.attachment.caption,
        replyTo: file.attachment.replyTo,
        thread: file.attachment.thread,
        ...(options ? { options } : {}),
      })
    : undefined;
  if (attachment && file.voice)
    throw new Error('Este arquivo tem dois tipos de envio. Selecione-o novamente.');
  if (file.bytes.length > MAX_FILE_BYTES)
    throw new Error('Este arquivo é grande demais para o navegador.');
  let framed: Uint8Array;
  try {
    framed = await this.core.bytes({
      kind: 'seal_file',
      name: file.name.replace(/[\\/\0]/g, '_').slice(0, 255) || 'arquivo',
      mime: file.mime || 'application/octet-stream',
      bytes: file.bytes,
      thumb: file.thumb ?? new Uint8Array(),
    });
  } catch {
    throw new Error('Não deu para preparar este arquivo.');
  }
  if (attachment) return this.sendAttachment(space, attachment, framed);
  const length = new DataView(framed.buffer, framed.byteOffset).getUint32(0, true);
  const head = object(JSON.parse(new TextDecoder().decode(framed.subarray(4, 4 + length))));
  const doc = parseFile(head.doc);
  if (file.voice) doc.voice = parseVoice(file.voice);
  const uploads = array(head.uploads).map((u) => {
    const x = object(u);
    return { blob: string(x.blob), len: integer(x.len) };
  });
  let offset = 4 + length;
  try {
    for (const [i, upload] of uploads.entries()) {
      await this.upload(upload.blob, framed.subarray(offset, offset + upload.len));
      offset += upload.len;
      progress?.(i + 1, uploads.length);
    }
    // A disconnect after the last PUT still must retain the recording locally.
    return await this.queueFileRecord(space, doc);
  } catch (error) {
    // Keep the sealed voice/file in the timeline as failed, like native — do not lose the clip.
    // Best effort: the upload error is what the user must see, even if queueing also fails.
    try {
      if (this.closed || !this.conversations.some((c) => c.id === space)) throw error;
      let start = 4 + length;
      const chunks: Record<string, number[]> = {};
      for (const upload of uploads) {
        chunks[upload.blob] = Array.from(framed.subarray(start, start + upload.len));
        start += upload.len;
      }
      const generation = this.generation;
      await this.enqueue(
        async () => {
          const update = await this.core.text({
            kind: 'queue_failed_file',
            space,
            doc: JSON.stringify(doc),
            now: Date.now(),
            reason:
              error instanceof Error
                ? error.message
                : 'Sem conexão para enviar o arquivo. Tente de novo.',
            chunks: JSON.stringify(chunks),
          });
          await this.apply(update, generation);
        },
        () => (this.closed ? 'A sessão mudou.' : null),
        false,
      );
    } catch {
      /* keep the original upload error */
    }
    throw error;
  }
}

/** All files and their exact ordered intent enter the existing outbox together. */
export async function sendFiles(
  this: MessageAttachmentsPort,
  space: string,
  files: readonly AttachmentBatchFile[],
  intent: AttachmentBatchIntent,
): Promise<readonly { itemId: string; messageId: string }[]> {
  const options = attachmentOptions(intent.options, files.length);
  let plainBytes = 0;
  const selected = files.map((file) => {
    if (
      !(file.bytes instanceof Uint8Array) ||
      !file.bytes.length ||
      file.bytes.length > (64 << 20) - plainBytes
    )
      throw new Error('O envio precisa ter até 64 MB.');
    plainBytes += file.bytes.length;
    if (
      file.thumb &&
      (!(file.thumb instanceof Uint8Array) || file.thumb.length > MAX_THUMBNAIL_BYTES)
    )
      throw new Error('Prévia inválida.');
    // Caller edits during asynchronous sealing cannot change the admitted intent or bytes.
    return {
      name: file.name.replace(/[\\/\0]/g, '_').slice(0, 255) || 'arquivo',
      mime: file.mime || 'application/octet-stream',
      bytes: Uint8Array.from(file.bytes),
      thumb: file.thumb ? Uint8Array.from(file.thumb) : new Uint8Array(),
      mutationId: file.mutationId,
      caption: file.caption,
    };
  });
  if (new Set(selected.map((file) => file.mutationId)).size !== selected.length)
    throw new Error('Seleção de arquivos inválida.');
  const request = JSON.stringify({
    space_id: space,
    reply_to: intent.replyTo,
    thread: intent.thread,
    ...(options ? { options } : {}),
    items: selected.map((file) => ({ mutation_id: file.mutationId, caption: file.caption })),
  });
  const frames: Uint8Array[] = [];
  let length = 0;
  for (const file of selected) {
    let frame: Uint8Array;
    try {
      frame = await this.core.bytes({
        kind: 'seal_file',
        name: file.name,
        mime: file.mime,
        bytes: file.bytes,
        thumb: file.thumb,
      });
    } catch {
      throw new Error('Não deu para preparar estes arquivos.');
    }
    if (frame.length < 4 || frame.length > (64 << 20) + MAX_THUMBNAIL_BYTES + (256 << 10))
      throw new Error('Arquivo preparado inválido.');
    length += 4 + frame.length;
    if (length > (64 << 20) + 10 * (MAX_THUMBNAIL_BYTES + (256 << 10) + 4))
      throw new Error('O envio é grande demais.');
    frames.push(frame);
  }
  const framed = new Uint8Array(length);
  const view = new DataView(framed.buffer);
  let offset = 0;
  for (const frame of frames) {
    view.setUint32(offset, frame.length, true);
    offset += 4;
    framed.set(frame, offset);
    offset += frame.length;
  }
  return this.sendAttachmentRequest('queue_attachment_batch', request, framed, (value) => {
    const posted = array(value);
    if (posted.length !== selected.length) throw new Error('Resposta de envio incompleta.');
    return Object.freeze(
      posted.map((input) => {
        const entry = object(input);
        return Object.freeze({ itemId: string(entry.itemId), messageId: string(entry.messageId) });
      }),
    );
  });
}

export async function newAttachmentMutationId(this: MessageAttachmentsPort): Promise<string> {
  let mutation = '';
  await this.enqueue(
    async () => {
      if (!this.attachmentProducer)
        this.attachmentProducer = await this.core.text({
          kind: 'attachment_producer',
          request: JSON.stringify({ op: 'begin' }),
        });
      mutation = await this.core.text({
        kind: 'attachment_producer',
        request: JSON.stringify({ op: 'issue', producer_id: this.attachmentProducer }),
      });
    },
    undefined,
    false,
  );
  return mutation;
}

export async function releaseAttachmentMutation(
  this: MessageAttachmentsPort,
  mutationId: string,
  accepted: boolean,
): Promise<void> {
  await this.enqueue(
    async () => {
      try {
        await this.core.text({
          kind: 'attachment_producer',
          request: JSON.stringify({ op: accepted ? 'accepted' : 'drop', mutation_id: mutationId }),
        });
      } catch (error) {
        throw Object.assign(new Error(message(error)), { soft: true });
      }
    },
    undefined,
    false,
  );
}

export async function sendAttachment(
  this: MessageAttachmentsPort,
  space: string,
  intent: AttachmentIntent,
  framed: Uint8Array,
): Promise<{ itemId: string; messageId: string }> {
  // Freeze exact metadata before entering the serial queue or crossing a reload.
  const request = JSON.stringify({
    space_id: space,
    mutation_id: intent.mutationId,
    caption: intent.caption,
    reply_to: intent.replyTo,
    thread: intent.thread,
    ...(intent.options ? { options: intent.options } : {}),
  });
  return this.sendAttachmentRequest('queue_attachment', request, framed, (value) => {
    const posted = object(value);
    return { itemId: string(posted.itemId), messageId: string(posted.messageId) };
  });
}

export async function sendAttachmentRequest<T>(
  this: MessageAttachmentsPort,
  kind: 'queue_attachment' | 'queue_attachment_batch',
  request: string,
  framed: Uint8Array,
  parse: (value: unknown) => T,
): Promise<T> {
  let result: T | undefined;
  await this.enqueue(
    async () => {
      const generation = this.generation;
      let response: Record<string, unknown>;
      try {
        response = object(
          JSON.parse(await this.core.text({ kind, request, framed, now: Date.now() })),
        );
      } catch (error) {
        throw Object.assign(new Error(plain(message(error))), { soft: true });
      }
      result = parse(response.result);
      // The existing encrypted snapshot owns the signed events, ciphertext and receipt.
      // No PUT or publish may precede this durable save.
      await this.apply(JSON.stringify(response.update), generation);
      if (!this.closed && generation === this.generation && this.connection.kind === 'ready') {
        try {
          await this.resumeDocumentUploads(generation);
        } catch (error) {
          if (!(error as { documentUploadRetry?: boolean }).documentUploadRetry) throw error;
          if (this.closed || generation !== this.generation) return;
          this.notice = 'Seu arquivo foi salvo e aguarda conexão para terminar de enviar.';
          this.scheduleMaintenance(5000, generation, true);
          this.publish();
        }
      }
    },
    () => (this.closed ? 'Esta sessão está fechada.' : null),
    false,
  );
  if (!result) throw new Error('Não foi possível guardar o arquivo para enviar.');
  return result;
}

export async function queueFileRecord(
  this: MessageAttachmentsPort,
  space: string,
  doc: FileDoc,
): Promise<{ itemId: string; messageId: string }> {
  if (this.closed) throw new Error('Esta sessão está fechada.');
  if (this.connection.kind !== 'ready')
    throw new Error('Sem conexão agora. Tente de novo em instantes.');
  let result: { itemId: string; messageId: string } | undefined;
  const generation = this.generation;
  await this.enqueue(
    async () => {
      let response: Record<string, unknown>;
      try {
        response = object(
          JSON.parse(
            await this.core.text({
              kind: 'queue_file_record',
              space,
              doc: JSON.stringify(doc),
              now: Date.now(),
            }),
          ),
        );
      } catch (error) {
        throw Object.assign(new Error(plain(message(error))), { soft: true });
      }
      const posted = object(response.result);
      result = { itemId: string(posted.itemId), messageId: string(posted.messageId) };
      await this.apply(JSON.stringify(response.update), generation);
    },
    () =>
      generation !== this.generation || this.connection.kind !== 'ready'
        ? 'A conexão mudou. Tente de novo.'
        : null,
    false,
  );
  if (!result) throw new Error('Não foi possível enviar o arquivo.');
  return result;
}

export async function fileManifest(
  this: MessageAttachmentsPort,
  itemId: string,
  version?: number,
): Promise<{ itemId: string; version: number; doc: FileDoc; info: unknown }> {
  const result = object(
    await this.documentRequest({ op: 'file_manifest', item_id: itemId, version: version ?? null }),
  );
  return {
    itemId: string(result.itemId),
    version: integer(result.version),
    doc: parseFile(result.doc),
    info: result.info,
  };
}

/** The same encrypted chunks, named again in another chat: nothing to upload. */
export function forwardFile(
  this: MessageAttachmentsPort,
  space: string,
  doc: FileDoc,
): Promise<void> {
  return this.act(space, {
    kind: 'queue_file',
    space,
    doc: JSON.stringify({ ...doc, voice: doc.voice ? { ...doc.voice, reply: null } : null }),
    now: Date.now(),
  });
}
