export const MAX_FILE_BYTES = 64 << 20;
const MAX_CHUNK = 16 << 20;
const MAX_JSON = 95_000_000;
export interface FileCore {
  fileManifest(item: string): Promise<string>;
  fileUploadHeaders(blob: string, now: number): Promise<string>;
  fileOpen(item: string, packed: Uint8Array): Promise<Uint8Array>;
  filePrepare(space: string, name: string, mime: string, bytes: Uint8Array, now: number): Promise<string>;
  fileCommit(space: string, planJSON: string, now: number, editId: string): Promise<string>;
}
export interface FileManifest { item: string; name: string; mime: string; bytes: number; chunks: { blob: string; bytes: number }[] }
export interface UploadPlan { schema: number; space: string; item: string; doc: FileManifest & Record<string, unknown>; uploads: { blob: string; body_base64: string; headers: Record<string, string> }[] }
export interface PreparedFile { plan: UploadPlan; planJSON: string; editId: string }
export interface FileNetwork { relay: string; fetch?: typeof globalThis.fetch }
export interface PageBlobCore {
  pageDownloadManifest(item: string): Promise<string>;
  pageBlobSupply(item: string, blob: string, sealed: Uint8Array): Promise<void>;
}
/** Supplies chunks through authenticated core; does not claim persistence or editability. */
export async function ensurePage(core: PageBlobCore, item: string, network: FileNetwork): Promise<void> {
  try {
    const manifest = json(await core.pageDownloadManifest(item));
    if (typeof manifest.ready !== 'boolean' || !Array.isArray(manifest.chunks) || manifest.chunks.length > 65) fail();
    const chunks = (manifest.chunks as unknown[]).map(value => {
      const chunk = record(value); const bytes = size(chunk.bytes, MAX_CHUNK);
      if (!bytes || chunk.sealed_bytes !== bytes + 40) fail();
      return { blob: hash(chunk.blob), bytes };
    });
    if (chunks.reduce((total, chunk) => total + chunk.bytes, 0) > MAX_FILE_BYTES) fail();
    const urls = chunks.map(chunk => endpoint(network.relay, chunk.blob));
    for (const [i, chunk] of chunks.entries()) {
      const response = await (network.fetch ?? globalThis.fetch)(urls[i]!, { credentials: 'omit', redirect: 'error' });
      if (!response.ok) fail();
      await core.pageBlobSupply(item, chunk.blob, await boundedBody(response, chunk.bytes + 40));
    }
  } catch { throw new Error('Page download failed; page readiness is unverified'); }
}
const fail = (): never => { throw new Error('Invalid or oversized file transfer'); };
const record = (v: unknown): Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v) ? v as Record<string, unknown> : fail();
const str = (v: unknown, max = 128): string => typeof v === 'string' && v.length > 0 && v.length <= max ? v : fail();
const hash = (v: unknown): string => typeof v === 'string' && /^[a-fA-F0-9]{64}$/.test(v) ? v : fail();
const size = (v: unknown, max: number): number => typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 && v <= max ? v : fail();
function json(raw: string): Record<string, unknown> { if (typeof raw !== 'string' || raw.length > MAX_JSON) fail(); try { return record(JSON.parse(raw)); } catch { return fail(); } }
function metadata(v: Record<string, unknown>): Omit<FileManifest, 'item'> {
  const name = str(v.name, 255); if (/[\\/\0]/.test(name)) fail();
  const mime = str(v.mime, 128); if (!mime.includes('/') || /[\r\n]/.test(mime)) fail();
  const bytes = size(v.bytes, MAX_FILE_BYTES);
  if (!Array.isArray(v.chunks) || v.chunks.length > 65) fail();
  const chunks = (v.chunks as unknown[]).map(c => { const r = record(c); const bytes = size(r.bytes, MAX_CHUNK); if (!bytes || (r.sealed_bytes !== undefined && r.sealed_bytes !== bytes + 40)) fail(); return { blob: hash(r.blob), bytes }; });
  if (chunks.reduce((n, c) => n + c.bytes, 0) !== bytes) fail();
  return { name, mime, bytes, chunks };
}
export function parseManifest(raw: string): FileManifest { const v = json(raw); return { item: str(v.item), ...metadata(v) }; }
export function parsePlan(planJSON: string, editId: string): PreparedFile {
  str(editId); const v = json(planJSON); const doc = record(v.doc); const meta = metadata(doc);
  if (v.schema !== 1 || !Array.isArray(v.uploads) || v.uploads.length !== meta.chunks.length) fail();
  hash(doc.sha256);
  // Check all chunks and encodings before allocating decoded ciphertext or doing network I/O.
  const uploads = (v.uploads as unknown[]).map((u, i) => {
    const r = record(u); const chunk = meta.chunks[i]; if (!chunk || hash(r.blob) !== chunk.blob) return fail();
    const originalChunk = record((doc.chunks as unknown[])[i]); hash(originalChunk.key); hash(originalChunk.sha256);
    const expected = chunk.bytes + 40;
    if (typeof r.body_base64 !== 'string' || r.body_base64.length !== Math.ceil(expected / 3) * 4 || !/^[A-Za-z0-9+/]*={0,2}$/.test(r.body_base64)) return fail();
    const padding = r.body_base64.endsWith('==') ? 2 : r.body_base64.endsWith('=') ? 1 : 0;
    if (r.body_base64.length / 4 * 3 - padding !== expected) fail();
    const h = record(r.headers); const headers: Record<string, string> = {};
    for (const key of ['x-zoen-device', 'x-zoen-ts', 'x-zoen-sig']) { const value = str(h[key], 2048); if (/[\r\n]/.test(value)) fail(); headers[key] = value; }
    return { blob: chunk.blob, body_base64: r.body_base64 as string, headers };
  });
  return { editId, planJSON, plan: { schema: 1, space: str(v.space), item: str(v.item), doc: { ...doc, ...meta, chunks: doc.chunks as FileManifest['chunks'], item: str(v.item) }, uploads } };
}
function endpoint(relay: string, blob: string): string {
  try { const u = new URL(relay); if (!['https:', 'http:'].includes(u.protocol) || u.username || u.password || u.search || u.hash) fail(); u.pathname = `${u.pathname.replace(/\/$/, '')}/v1/blobs/${encodeURIComponent(blob)}`; return u.href; } catch { return fail(); }
}
async function boundedBody(response: Response, expected: number): Promise<Uint8Array> {
  const length = response.headers.get('content-length'); if (length !== null && Number(length) !== expected) fail();
  if (!response.body) { if (expected) fail(); return new Uint8Array(); }
  const reader = response.body.getReader(); const bytes = new Uint8Array(expected); let offset = 0;
  try { for (;;) { const part = await reader.read(); if (part.done) break; if (part.value.length > expected - offset) { await reader.cancel(); fail(); } bytes.set(part.value, offset); offset += part.value.length; } } finally { reader.releaseLock(); }
  if (offset !== expected) fail(); return bytes;
}
export async function download(core: FileCore, item: string, network: FileNetwork): Promise<{ name: string; mime: string; bytes: Uint8Array }> {
  try {
    const manifest = parseManifest(await core.fileManifest(item)); if (manifest.item !== item) fail();
    const urls = manifest.chunks.map(c => endpoint(network.relay, c.blob));
    const packed = new Uint8Array(manifest.bytes + manifest.chunks.length * 40); let offset = 0;
    for (const [i, chunk] of manifest.chunks.entries()) {
      const response = await (network.fetch ?? globalThis.fetch)(urls[i]!, { credentials: 'omit', redirect: 'error' });
      if (!response.ok) throw new Error('Download refused');
      const bytes = await boundedBody(response, chunk.bytes + 40); packed.set(bytes, offset); offset += bytes.length;
    }
    const bytes = await core.fileOpen(item, packed); if (!(bytes instanceof Uint8Array) || bytes.length !== manifest.bytes) fail();
    return { name: manifest.name, mime: manifest.mime, bytes };
  } catch { throw new Error('File download failed; no verified file is available'); }
}
export async function prepare(core: FileCore, space: string, name: string, mime: string, bytes: Uint8Array, now: number, editId = crypto.randomUUID()): Promise<PreparedFile> {
  try { if (!(bytes instanceof Uint8Array) || bytes.length > MAX_FILE_BYTES) fail(); const prepared = parsePlan(await core.filePrepare(space, name, mime, bytes, now), editId); if (prepared.plan.space !== space || prepared.plan.doc.bytes !== bytes.length) fail(); return prepared; } catch { throw new Error('File preparation failed'); }
}
/** Upload only. The caller must durably retain the encrypted plan before calling,
 * then commit/persist/send only after this promise resolves. No persistence here. */
export async function upload(core: FileCore, prepared: PreparedFile, network: FileNetwork, clock: () => number = Date.now): Promise<void> {
  try {
    const { plan } = parsePlan(prepared.planJSON, prepared.editId);
    const urls = plan.uploads.map(u => endpoint(network.relay, u.blob));
    for (const [i, chunk] of plan.uploads.entries()) {
      const body = Uint8Array.from(atob(chunk.body_base64), c => c.charCodeAt(0));
      const fresh = json(await core.fileUploadHeaders(chunk.blob, clock()));
      const headers: Record<string, string> = {};
      for (const key of ['x-zoen-device', 'x-zoen-ts', 'x-zoen-sig']) { const value = str(fresh[key], 2048); if (/[\r\n]/.test(value)) fail(); headers[key] = value; }
      const response = await (network.fetch ?? globalThis.fetch)(urls[i]!, { method: 'PUT', body, headers, credentials: 'omit', redirect: 'error' });
      if (!response.ok) throw new Error('Upload refused');
    }
  } catch { throw new Error('File upload failed; file was not committed'); }
}
