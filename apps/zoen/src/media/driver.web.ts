import { getBrowserMediaHost, type BrowserMediaHost } from '../core/browser-media.ts';
import type { FileDto } from '../core/generated/FileDto';
import { t } from '../i18n/index.ts';
import { MAX_SELECTED_BYTES, type MediaDriver } from './types.ts';

function current(host: BrowserMediaHost, resolveHost: () => BrowserMediaHost) {
  if (host.closed.aborted || resolveHost() !== host)
    throw new Error('A sessão mudou. Abra esta conversa novamente.');
}

/** Read only a selected local Blob; never fetch an attachment from a caller's remote URL. */
async function selectedBytes(
  uri: string,
  host: BrowserMediaHost,
  resolveHost: () => BrowserMediaHost,
) {
  const url = new URL(uri);
  if (url.protocol !== 'blob:' || url.origin !== window.location.origin)
    throw new Error('Selecione este arquivo novamente.');
  current(host, resolveHost);
  const response = await fetch(url, { signal: host.closed });
  if (!response.ok || !response.body) throw new Error('Não foi possível ler este arquivo.');
  const declared = Number(response.headers.get('content-length'));
  if (declared > MAX_SELECTED_BYTES) {
    await response.body.cancel();
    throw new Error('Escolha um arquivo de até 64 MB.');
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value.byteLength > MAX_SELECTED_BYTES - length)
        throw new Error('Escolha um arquivo de até 64 MB.');
      length += value.byteLength;
      chunks.push(value);
    }
  } finally {
    await reader.cancel().catch(() => {});
  }
  if (!length) throw new Error('Este arquivo está vazio.');
  current(host, resolveHost);
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return { bytes, mime: response.headers.get('content-type') ?? '' };
}

function selectionMetadata(name: string, mime: string) {
  if (
    !name ||
    /[\\/\0]/.test(name) ||
    new TextEncoder().encode(name).length > 255 ||
    !mime.includes('/') ||
    mime.length > 128
  )
    throw new Error('O nome ou tipo deste arquivo não é válido. Selecione-o novamente.');
}

function previewInfo(info: FileDto): FileDto {
  if (!/^[0-9]+$/.test(info.bytes) || BigInt(info.bytes) > BigInt(MAX_SELECTED_BYTES))
    throw new Error('Escolha um arquivo de até 64 MB.');
  return info;
}

export function createBrowserMediaDriver(
  resolveHost: () => BrowserMediaHost = getBrowserMediaHost,
): MediaDriver {
  const owned = new Map<string, BrowserMediaHost>();
  const observed = new WeakSet<BrowserMediaHost>();
  function observe(host: BrowserMediaHost) {
    if (observed.has(host)) return;
    observed.add(host);
    host.closed.addEventListener(
      'abort',
      () => {
        for (const [uri, owner] of owned)
          if (owner === host) {
            URL.revokeObjectURL(uri);
            owned.delete(uri);
          }
      },
      { once: true },
    );
  }
  return {
    async sendFile(selection) {
      if (selection.replyTo || selection.thread) throw new Error(t('media.fileReplyUnavailable'));
      if (selection.caption?.trim()) throw new Error(t('media.fileCaptionUnavailable'));
      const host = resolveHost();
      selectionMetadata(selection.name, selection.mime);
      const local = await selectedBytes(selection.uri, host, resolveHost);
      const result = await host.sendFile(selection.spaceId, {
        name: selection.name,
        mime: selection.mime,
        bytes: local.bytes,
      });
      current(host, resolveHost);
      return result;
    },
    async sendVoice(recording) {
      const host = resolveHost();
      if (
        !Number.isSafeInteger(recording.durationMs) ||
        recording.durationMs < 250 ||
        recording.durationMs > 3_600_000 ||
        recording.levels.some((level) => !Number.isFinite(level) || level < 0 || level > 1)
      )
        throw new Error('Esta gravação está vazia ou inválida. Grave novamente.');
      const local = await selectedBytes(recording.uri, host, resolveHost);
      // Safari may choose AAC/MP4 when WebM is unsupported. Preserve the actual
      // recorder Blob's MIME type instead of labelling its bytes as WebM.
      const mime = local.mime || recording.mime;
      if (!mime.startsWith('audio/'))
        throw new Error('Este arquivo não contém uma gravação de áudio.');
      const extension = mime.includes('mp4')
        ? 'm4a'
        : mime.includes('ogg')
          ? 'ogg'
          : mime.includes('mpeg')
            ? 'mp3'
            : 'webm';
      const result = await host.sendFile(recording.spaceId, {
        name: `voice-${crypto.randomUUID()}.${extension}`,
        mime,
        bytes: local.bytes,
        voice: {
          ms: recording.durationMs,
          levels: recording.levels
            .slice(0, 160)
            .map((level) => Math.round(level * 15).toString(16))
            .join(''),
          transcript: '',
          reply: recording.replyTo ? { to: recording.replyTo, thread: recording.thread } : null,
        },
      });
      current(host, resolveHost);
      return result;
    },
    async fileInfo(itemId, version) {
      const host = resolveHost();
      const file = await host.file(itemId, version);
      current(host, resolveHost);
      return file.info;
    },
    async materializeFile(itemId, version) {
      const host = resolveHost();
      observe(host);
      // The signed manifest and exact version travel together through Rust.
      const file = await host.file(itemId, version);
      previewInfo(file.info);
      current(host, resolveHost);
      const blob = await host.openFile(file.doc);
      current(host, resolveHost);
      if (blob.size !== Number(file.info.bytes) || blob.size > MAX_SELECTED_BYTES)
        throw new Error('O tamanho deste arquivo não corresponde à versão assinada.');
      const uri = URL.createObjectURL(blob);
      owned.set(uri, host);
      return { uri, itemId: file.itemId, version: file.version, info: file.info };
    },
    async releaseLocalFile(uri) {
      if (owned.delete(uri)) URL.revokeObjectURL(uri);
    },
  };
}
