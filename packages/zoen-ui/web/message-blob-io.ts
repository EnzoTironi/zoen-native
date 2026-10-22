// Whole operations borrowed from the sole Messages owner.
import { type FileDoc, boundedFile, MAX_CHUNK_BYTES } from './message-file-model.ts';
import { hex } from './boundary.ts';
import type { MessageOperations } from './message-operations.ts';
type MessageBlobIoPort = Pick<
  MessageOperations,
  'core' | 'transport' | 'vault' | 'base' | 'aborted' | 'session'
>;

export async function upload(
  this: MessageBlobIoPort,
  blob: string,
  sealed: Uint8Array,
): Promise<void> {
  for (let attempt = 0; ; attempt++) {
    const ts = Date.now();
    const payload = await this.core.bytes({
      kind: 'blob_put_message',
      sha: blob,
      ts,
      relay: this.transport.relayName,
    });
    const sig = Array.from(await this.vault.sign(payload), (b) =>
      b.toString(16).padStart(2, '0'),
    ).join('');
    try {
      const response = await fetch(`${this.base()}/v1/blobs/${blob}`, {
        method: 'PUT',
        body: Uint8Array.from(sealed),
        signal: this.aborted.signal,
        headers: {
          'content-type': 'application/octet-stream',
          'x-zoen-device': this.session.account.device,
          'x-zoen-ts': String(ts),
          'x-zoen-sig': sig,
        },
      });
      if (response.ok) return;
      if (response.status === 413)
        throw Object.assign(new Error('Este arquivo é grande demais.'), { final: true });
      if (response.status < 500 && response.status !== 429)
        throw Object.assign(new Error('O servidor recusou o arquivo.'), { final: true });
      throw new Error('O servidor não conseguiu receber o arquivo.');
    } catch (error) {
      if ((error as { final?: boolean }).final || attempt >= 3)
        throw error instanceof Error && (error as { final?: boolean }).final
          ? error
          : new Error('Sem conexão para enviar o arquivo. Tente de novo.');
    }
    await new Promise((r) => setTimeout(r, 800 * 2 ** attempt));
  }
}

/** Fetches, checks and opens a file's chunks. Throws plain Portuguese on failure. */
export async function openFile(
  this: MessageBlobIoPort,
  input: FileDoc,
  thumbnail: boolean,
  pagePayload: boolean,
): Promise<Blob> {
  const file = boundedFile(input, pagePayload ? 32 << 20 : MAX_CHUNK_BYTES);
  const expected = thumbnail ? file.thumb : file;
  if (!expected) throw new Error('Este arquivo não tem prévia.');
  const chunks = thumbnail ? [file.thumb!] : file.chunks;
  const plain = new Uint8Array(expected.bytes);
  let offset = 0;
  for (const chunk of chunks) {
    let sealed: Uint8Array;
    try {
      const response = await fetch(`${this.base()}/v1/blobs/${chunk.blob}`, {
        signal: this.aborted.signal,
      });
      if (!response.ok || !response.body) {
        await response.body?.cancel();
        throw new Error(String(response.status));
      }
      const reader = response.body.getReader();
      sealed = new Uint8Array(chunk.bytes + 40); // XChaCha20 nonce + authentication tag.
      let received = 0;
      try {
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          if (value.length > sealed.length - received) throw new Error('Invalid chunk length.');
          sealed.set(value, received);
          received += value.length;
        }
        if (received !== sealed.length) throw new Error('Invalid chunk length.');
      } finally {
        await reader.cancel().catch(() => {});
      }
    } catch {
      sealed = await this.core.bytes({ kind: 'local_chunk', blob: chunk.blob });
      if (!sealed.length)
        throw new Error('O arquivo ainda está chegando. Tente de novo em instantes.');
    }
    try {
      const opened = await this.core.bytes({
        kind: 'open_chunk',
        chunk: JSON.stringify(chunk),
        sealed,
      });
      if (opened.length !== chunk.bytes) throw new Error('Invalid chunk length.');
      plain.set(opened, offset);
      offset += opened.length;
    } catch {
      throw new Error('Não deu para abrir este arquivo.');
    }
  }
  if (
    offset !== expected.bytes ||
    hex(new Uint8Array(await crypto.subtle.digest('SHA-256', plain))) !== expected.sha256
  ) {
    throw new Error('Não deu para abrir este arquivo.');
  }
  return new Blob([plain], { type: thumbnail ? 'image/png' : file.mime });
}
