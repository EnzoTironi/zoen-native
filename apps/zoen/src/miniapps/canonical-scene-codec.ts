import { sha256 } from '@noble/hashes/sha2.js';
import {
  MAX_CANONICAL_SCENE_BYTES,
  CANONICAL_SCENE_CHUNK_BYTES,
  type CanonicalSceneDoc,
} from './canonical-scene.ts';
export const token = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
export const digest = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
export const hash = (value: Uint8Array) =>
  Array.from(sha256(value), (byte) => byte.toString(16).padStart(2, '0')).join('');
export const integer = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value);
const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const encoder = new TextEncoder();

/** Count first: untrusted UTF-16 must never cause an oversized UTF-8 allocation. */
export function utf8Length(value: string): number {
  let bytes = 0;
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code < 0x80) bytes++;
    else if (code < 0x800) bytes += 2;
    else if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(++index);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw new Error('Invalid scene UTF-8.');
      bytes += 4;
    } else if (code >= 0xdc00 && code <= 0xdfff) throw new Error('Invalid scene UTF-8.');
    else bytes += 3;
    if (bytes > MAX_CANONICAL_SCENE_BYTES) throw new Error('The scene exceeds its byte limit.');
  }
  return bytes;
}
export function encode64(bytes: Uint8Array): string {
  const parts: string[] = [];
  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i];
    const b = bytes[i + 1];
    const c = bytes[i + 2];
    parts.push(
      alphabet[a >> 2] +
        alphabet[((a & 3) << 4) | ((b ?? 0) >> 4)] +
        (b === undefined ? '=' : alphabet[((b & 15) << 2) | ((c ?? 0) >> 6)]) +
        (c === undefined ? '=' : alphabet[c & 63]),
    );
  }
  return parts.join('');
}
export function decodeUtf8(bytes: Uint8Array): string {
  // Validate independently of TextDecoder's platform-specific fatal option.
  for (let i = 0; i < bytes.length;) {
    const a = bytes[i++];
    if (a < 0x80) continue;
    const b = bytes[i++];
    if (a >= 0xc2 && a <= 0xdf && b >= 0x80 && b <= 0xbf) continue;
    const c = bytes[i++];
    if (
      a >= 0xe0 &&
      a <= 0xef &&
      b >= (a === 0xe0 ? 0xa0 : 0x80) &&
      b <= (a === 0xed ? 0x9f : 0xbf) &&
      c >= 0x80 &&
      c <= 0xbf
    )
      continue;
    const d = bytes[i++];
    if (
      a >= 0xf0 &&
      a <= 0xf4 &&
      b >= (a === 0xf0 ? 0x90 : 0x80) &&
      b <= (a === 0xf4 ? 0x8f : 0xbf) &&
      c >= 0x80 &&
      c <= 0xbf &&
      d >= 0x80 &&
      d <= 0xbf
    )
      continue;
    throw new Error('Invalid scene UTF-8.');
  }
  return new TextDecoder().decode(bytes);
}
export function decode64(value: unknown): Uint8Array {
  if (
    typeof value !== 'string' ||
    !value.length ||
    value.length % 4 !== 0 ||
    value.length > (CANONICAL_SCENE_CHUNK_BYTES / 3) * 4
  )
    throw new Error('Invalid scene chunk base64.');
  const padding = value.endsWith('==') ? 2 : value.endsWith('=') ? 1 : 0;
  const output = new Uint8Array((value.length / 4) * 3 - padding);
  let offset = 0;
  for (let index = 0; index < value.length; index += 4) {
    const last = index + 4 === value.length;
    const a = alphabet.indexOf(value[index]);
    const b = alphabet.indexOf(value[index + 1]);
    const c = value[index + 2] === '=' && last ? 0 : alphabet.indexOf(value[index + 2]);
    const d = value[index + 3] === '=' && last ? 0 : alphabet.indexOf(value[index + 3]);
    if (
      a < 0 ||
      b < 0 ||
      c < 0 ||
      d < 0 ||
      (value[index + 2] === '=' && value[index + 3] !== '=') ||
      (last && padding === 2 && (b & 15) !== 0) ||
      (last && padding === 1 && (c & 3) !== 0)
    )
      throw new Error('Invalid scene chunk base64.');
    output[offset++] = (a << 2) | (b >> 4);
    if (offset < output.length) output[offset++] = (b << 4) | (c >> 2);
    if (offset < output.length) output[offset++] = (c << 6) | d;
  }
  return output;
}
export function metadata(doc: CanonicalSceneDoc) {
  if (
    !token(doc.item_id) ||
    !integer(doc.version) ||
    doc.version < 1 ||
    !integer(doc.current_version) ||
    doc.current_version < doc.version ||
    doc.current_version > 0xffffffff ||
    !digest(doc.sha256) ||
    typeof doc.scene_json !== 'string'
  )
    throw new Error('Invalid canonical scene document.');
}
export function validateDoc(doc: CanonicalSceneDoc): CanonicalSceneDoc {
  metadata(doc);
  if (!utf8Length(doc.scene_json) || hash(encoder.encode(doc.scene_json)) !== doc.sha256)
    throw new Error('The canonical scene hash does not match its bytes.');
  return Object.freeze({
    item_id: doc.item_id,
    version: doc.version,
    sha256: doc.sha256,
    scene_json: doc.scene_json,
    current_version: doc.current_version,
  });
}
