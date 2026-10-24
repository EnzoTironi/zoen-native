import { sha256 } from '@noble/hashes/sha2.js';

export type RuntimeAsset = Readonly<{
  url: string;
  bytes: number;
  sha256: string;
  mime: 'text/javascript' | 'application/wasm';
}>;
export type RuntimePackage = Readonly<{
  schema: 1;
  engine: 'deckcraft' | 'gridcraft' | 'wordcraft';
  revision: string;
  abi: 'zoen-host-v1';
  module: RuntimeAsset;
  wasm: RuntimeAsset;
}>;
export type RuntimeAssets = Readonly<{ module: string; wasm: Uint8Array<ArrayBuffer> }>;
const HASH = /^[a-f0-9]{64}$/;
const MODULE_LIMIT = 512 * 1024;
const WASM_LIMIT = 48 * 1024 * 1024;
const invalid = () => new Error('Invalid verified App runtime package');
function record(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (
    !value ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    Object.keys(value).sort().join(',') !== [...keys].sort().join(',')
  )
    throw invalid();
  return value as Record<string, unknown>;
}
export function runtimeAssetUrl(value: unknown): URL {
  if (typeof value !== 'string' || value.length > 2048) throw invalid();
  const url = new URL(value);
  if (
    url.protocol !== 'https:' ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.href !== value ||
    !/^\/[a-zA-Z0-9/_.,-]+$/.test(url.pathname) ||
    url.pathname.includes('//') ||
    url.pathname.split('/').some((part) => part === '.' || part === '..')
  )
    throw invalid();
  return url;
}
/** Code resources are pinned separately from the unchanged 8MiB HTML allowance. */
export function parseRuntimePackage(value: unknown): RuntimePackage {
  const input = record(value, ['schema', 'engine', 'revision', 'abi', 'module', 'wasm']);
  if (
    input.schema !== 1 ||
    (input.engine !== 'deckcraft' &&
      input.engine !== 'gridcraft' &&
      input.engine !== 'wordcraft') ||
    input.abi !== 'zoen-host-v1' ||
    typeof input.revision !== 'string' ||
    !/^[a-f0-9]{40}$/.test(input.revision)
  )
    throw invalid();
  const asset = (value: unknown, mime: RuntimeAsset['mime'], max: number): RuntimeAsset => {
    const input = record(value, ['url', 'bytes', 'sha256', 'mime']);
    const url = runtimeAssetUrl(input.url);
    if (
      input.mime !== mime ||
      !Number.isSafeInteger(input.bytes) ||
      (input.bytes as number) < (mime === 'application/wasm' ? 8 : 1) ||
      (input.bytes as number) > max ||
      typeof input.sha256 !== 'string' ||
      !HASH.test(input.sha256) ||
      !url.pathname.endsWith(mime === 'application/wasm' ? '.wasm' : '.js')
    )
      throw invalid();
    return Object.freeze({
      url: url.href,
      bytes: input.bytes as number,
      sha256: input.sha256,
      mime,
    });
  };
  const module = asset(input.module, 'text/javascript', MODULE_LIMIT);
  const wasm = asset(input.wasm, 'application/wasm', WASM_LIMIT);
  const moduleUrl = new URL(module.url);
  const wasmUrl = new URL(wasm.url);
  const directory = moduleUrl.pathname.slice(0, moduleUrl.pathname.lastIndexOf('/') + 1);
  if (
    moduleUrl.origin !== wasmUrl.origin ||
    wasmUrl.pathname.slice(0, wasmUrl.pathname.lastIndexOf('/') + 1) !== directory ||
    !directory.endsWith(`/${input.revision}/`)
  )
    throw invalid();
  return Object.freeze({
    schema: 1,
    engine: input.engine,
    revision: input.revision,
    abi: 'zoen-host-v1',
    module,
    wasm,
  });
}

/** Trusted parent only. No child-provided URLs, headers, credentials or document data. */
export function createRuntimePackageLoader(
  input: RuntimePackage,
  check: () => void,
  fetchAsset: typeof fetch = globalThis.fetch,
) {
  const runtime = parseRuntimePackage(input);
  const abort = new AbortController();
  let active = true;
  let pending: Promise<RuntimeAssets> | undefined;
  const current = () => {
    if (!active) throw new Error('App runtime retired');
    check();
  };
  async function read(asset: RuntimeAsset): Promise<Uint8Array<ArrayBuffer>> {
    current();
    const response = await fetchAsset(asset.url, {
      method: 'GET',
      credentials: 'omit',
      redirect: 'error',
      cache: 'no-store',
      referrerPolicy: 'no-referrer',
      mode: 'cors',
      signal: abort.signal,
    });
    try {
      current();
    } catch (error) {
      await response.body?.cancel().catch(() => {});
      throw error;
    }
    if (
      response.status !== 200 ||
      response.redirected ||
      response.url !== asset.url ||
      !['basic', 'cors', 'default'].includes(response.type) ||
      response.headers.get('content-type')?.split(';', 1)[0]?.trim().toLowerCase() !== asset.mime ||
      !response.body
    ) {
      await response.body?.cancel();
      throw invalid();
    }
    const length = response.headers.get('content-length');
    if (length !== null && (!/^\d+$/.test(length) || Number(length) > asset.bytes)) {
      await response.body.cancel();
      throw invalid();
    }
    // One exact allocation; never concatenate an unbounded or encoded response.
    const bytes = new Uint8Array(asset.bytes);
    const digest = sha256.create();
    const reader = response.body.getReader();
    let size = 0;
    try {
      while (true) {
        const chunk = await reader.read();
        current();
        if (chunk.done) break;
        if (!(chunk.value instanceof Uint8Array) || size + chunk.value.length > asset.bytes)
          throw invalid();
        bytes.set(chunk.value, size);
        digest.update(chunk.value);
        size += chunk.value.length;
      }
      const actual = Array.from(digest.digest(), (n) => n.toString(16).padStart(2, '0')).join('');
      if (size !== asset.bytes || actual !== asset.sha256) throw invalid();
      current();
      return bytes;
    } finally {
      digest.destroy();
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  }
  return {
    load(): Promise<RuntimeAssets> {
      current();
      return (pending ??= (async () => {
        const module = await read(runtime.module);
        const wasm = await read(runtime.wasm);
        current();
        if (!wasm.subarray(0, 8).every((n, i) => n === [0, 97, 115, 109, 1, 0, 0, 0][i]))
          throw invalid();
        return Object.freeze({
          module: new TextDecoder('utf-8', { fatal: true }).decode(module),
          wasm,
        });
      })());
    },
    dispose() {
      active = false;
      abort.abort();
    },
  };
}
