import {
  binaryExportAllowed,
  type BinaryEditorId,
  type BinaryExportFormat,
} from './binary-formats.ts';
import { sha256 } from '@noble/hashes/sha2.js';
import { decode64 } from './canonical-scene.ts';

/** Kept for existing Deck callers; trusted editor selection controls the actual set. */
export type DeckcraftExportFormat = BinaryExportFormat;
export type DeckcraftExportHead = { version: string; sha256: string };
export type DeckcraftExportFile = {
  write: (bytes: Uint8Array) => Promise<void>;
  finish: () => number;
  share: () => Promise<void>;
  dispose: () => void;
};
export type DeckcraftExportOptions = {
  editor?: BinaryEditorId;
  title: string;
  format: DeckcraftExportFormat;
  head: DeckcraftExportHead;
  check: () => void;
  call: (name: string, args: Record<string, unknown>) => Promise<unknown>;
  create: (filename: string, format: DeckcraftExportFormat) => DeckcraftExportFile;
};
const CHUNK = 196_608;
const MAX = 16 * 1024 * 1024;
const token = (v: unknown): v is string =>
  typeof v === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(v);
const digest = (v: unknown): v is string => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw Error('Invalid presentation export.');
  return value as Record<string, unknown>;
}
function result(value: unknown) {
  const reply = record(value);
  if (reply.isError) throw Error('The presentation export was refused.');
  return record(reply.structuredContent);
}
export function deckcraftExportFilename(
  title: string,
  head: DeckcraftExportHead,
  format: DeckcraftExportFormat,
) {
  const name =
    title
      .replace(/[^\p{L}\p{N} _.-]/gu, '')
      .replace(/^\.+/, '')
      .trim()
      .slice(0, 80) ||
    (format === 'xlsx' ? 'workbook' : format === 'docx' ? 'document' : 'presentation');
  return `${name}-v${head.version}.${format}`;
}

/** Exact accepted-head bytes are streamed to one private temporary file. Neither
 * a full export base64 string nor an unverified file reaches the native share UI. */
export async function streamDeckcraftExport(options: DeckcraftExportOptions): Promise<void> {
  options.check();
  if (
    !binaryExportAllowed(options.editor ?? 'deckcraft', options.format) ||
    !/^[1-9][0-9]{0,9}$/.test(options.head.version) ||
    !digest(options.head.sha256)
  )
    throw Error('Invalid accepted presentation version.');
  let transfer: string | undefined;
  let file: DeckcraftExportFile | undefined;
  try {
    const metadata = result(
      await options.call('prepare_document_export', { format: options.format, head: options.head }),
    );
    options.check();
    if (token(metadata.transferId)) transfer = metadata.transferId;
    const source = record(metadata.sourceHead);
    if (
      !transfer ||
      metadata.format !== options.format ||
      source.version !== options.head.version ||
      source.sha256 !== options.head.sha256 ||
      !digest(metadata.sha256) ||
      !Number.isSafeInteger(metadata.bytes) ||
      Number(metadata.bytes) < 1 ||
      Number(metadata.bytes) > MAX ||
      metadata.byteLength !== metadata.bytes ||
      metadata.chunkBytes !== CHUNK
    )
      throw Error('Invalid accepted presentation export.');
    const bytes = Number(metadata.bytes);
    file = options.create(
      deckcraftExportFilename(options.title, options.head, options.format),
      options.format,
    );
    const hash = sha256.create();
    for (let index = 0, written = 0; written < bytes; index++) {
      const chunk = result(
        await options.call('read_document_export_chunk', { transferId: transfer, index }),
      );
      options.check();
      if (chunk.index !== index) throw Error('Invalid presentation export chunk order.');
      const data = decode64(chunk.data);
      try {
        if (data.length !== Math.min(CHUNK, bytes - written))
          throw Error('Invalid presentation export chunk length.');
        if (
          !index &&
          (options.format === 'pdf'
            ? String.fromCharCode(...data.subarray(0, 5)) !== '%PDF-'
            : data[0] !== 0x50 || data[1] !== 0x4b || data[2] !== 3 || data[3] !== 4)
        )
          throw Error('Invalid presentation export format.');
        hash.update(data);
        await file.write(data);
        options.check();
        written += data.length;
      } finally {
        data.fill(0);
      }
    }
    const actual = Array.from(hash.digest(), (value) => value.toString(16).padStart(2, '0')).join(
      '',
    );
    if (actual !== metadata.sha256 || file.finish() !== bytes)
      throw Error('The presentation export bytes changed.');
    options.check();
    await file.share();
    options.check();
  } finally {
    try {
      file?.dispose();
    } finally {
      if (transfer)
        await options.call('release_document_export', { transferId: transfer }).catch(() => {});
    }
  }
}
