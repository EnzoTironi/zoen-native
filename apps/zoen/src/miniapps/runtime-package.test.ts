import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import {
  createRuntimePackageLoader,
  parseRuntimePackage,
  type RuntimePackage,
} from './runtime-package.ts';
import { createRuntimeHost, installRuntimeClient } from './runtime-browser.ts';
import { miniappCsp, sandboxHtml, verifyMiniappResource } from './sandbox.ts';
import { nativeMiniappHtml } from './host-transport.ts';

const revision = 'a'.repeat(40);
const root = `https://code.example.test/deckcraft/${revision}/`;
const hash = (bytes: Uint8Array | string) => createHash('sha256').update(bytes).digest('hex');
const wasm = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);
const moduleText =
  'export default async()=>{};export function zoenHostStart(){};export function zoenHostRetire(){}';
function fixture(module = moduleText) {
  const runtime = parseRuntimePackage({
    schema: 1,
    engine: 'deckcraft',
    revision,
    abi: 'zoen-host-v1',
    module: {
      url: root + 'engine.js',
      bytes: Buffer.byteLength(module),
      sha256: hash(module),
      mime: 'text/javascript',
    },
    wasm: {
      url: root + 'engine.wasm',
      bytes: wasm.length,
      sha256: hash(wasm),
      mime: 'application/wasm',
    },
  });
  return { runtime, module };
}
function response(
  asset: RuntimePackage['module'],
  bytes: Uint8Array | string,
  extra: Record<string, unknown> = {},
) {
  const result = new Response(typeof bytes === 'string' ? bytes : Uint8Array.from(bytes), {
    headers: { 'content-type': asset.mime },
  });
  Object.defineProperties(result, {
    url: { value: asset.url },
    ...Object.fromEntries(Object.entries(extra).map(([key, value]) => [key, { value }])),
  });
  return result;
}
function network(runtime: RuntimePackage, module = moduleText) {
  const calls: { url: string; options?: RequestInit }[] = [];
  const fetcher = (async (url, options) => {
    calls.push({ url: String(url), options });
    return String(url) === runtime.module.url
      ? response(runtime.module, module)
      : response(runtime.wasm, wasm);
  }) as typeof fetch;
  return { calls, fetcher };
}
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));

test('lazy exact-asset fetch pins immutable metadata and never receives document or child URL data', async () => {
  const { runtime } = fixture();
  const { calls, fetcher } = network(runtime);
  const loader = createRuntimePackageLoader(runtime, () => {}, fetcher);
  assert.equal(calls.length, 0);
  const first = loader.load();
  assert.equal(first, loader.load());
  const result = await first;
  assert.deepEqual(result.wasm, wasm);
  assert.equal(result.module, moduleText);
  assert.deepEqual(
    calls.map((x) => x.url),
    [runtime.module.url, runtime.wasm.url],
  );
  for (const { options } of calls) {
    assert.equal(options?.credentials, 'omit');
    assert.equal(options?.redirect, 'error');
    assert.equal(options?.cache, 'no-store');
    assert.equal(options?.referrerPolicy, 'no-referrer');
    assert.equal(options?.method, 'GET');
    assert.equal(options?.body, undefined);
    assert.equal(options?.headers, undefined);
  }
  for (const item of [runtime, runtime.module, runtime.wasm])
    assert.equal(Object.isFrozen(item), true);
});

test('package authority rejects unknown keys, origin/path/query changes, bad ABI and independent per-asset budgets', () => {
  const { runtime } = fixture();
  for (const change of [
    { schema: 2 },
    { engine: 'other' },
    { abi: 'standalone' },
    { revision: 'main' },
    { extra: true },
  ])
    assert.throws(() => parseRuntimePackage({ ...runtime, ...change }));
  for (const url of [
    'http://example.test/a.js',
    `http://127.0.0.1:17431/deckcraft/${revision}/engine.js`,
    `http://[::1]:17431/deckcraft/${revision}/engine.js`,
    runtime.module.url + '?document=secret',
    runtime.module.url + '#x',
    root + '%65ngine.js',
    root + '../engine.js',
    'https://user:pass@code.example.test/a.js',
  ])
    assert.throws(() => parseRuntimePackage({ ...runtime, module: { ...runtime.module, url } }));
  for (const bytes of [0, 512 * 1024 + 1, NaN, 1.5])
    assert.throws(() => parseRuntimePackage({ ...runtime, module: { ...runtime.module, bytes } }));
  assert.throws(() =>
    parseRuntimePackage({ ...runtime, wasm: { ...runtime.wasm, bytes: 48 * 1024 * 1024 + 1 } }),
  );
  assert.throws(() =>
    parseRuntimePackage({
      ...runtime,
      wasm: {
        ...runtime.wasm,
        url: runtime.wasm.url.replace('code.example.test', 'other.example.test'),
      },
    }),
  );
  assert.throws(() =>
    parseRuntimePackage({
      ...runtime,
      wasm: { ...runtime.wasm, url: runtime.wasm.url.replace(`/${revision}/`, '/other/') },
    }),
  );
});

test('same-size corruption, short/excess bodies, redirects, MIME and final URL mismatches fail before execution', async () => {
  const { runtime } = fixture();
  for (const factory of [
    () => response(runtime.module, moduleText.replace('async', 'Async')),
    () => response(runtime.module, moduleText.slice(1)),
    () => response(runtime.module, moduleText + 'x'),
    () => response(runtime.module, moduleText, { redirected: true }),
    () => response(runtime.module, moduleText, { url: runtime.module.url + '?changed' }),
    () => response(runtime.module, moduleText, { type: 'opaque' }),
    () => {
      const r = response(runtime.module, moduleText);
      r.headers.set('content-type', 'text/html');
      return r;
    },
    () => {
      const r = response(runtime.module, moduleText);
      r.headers.set('content-length', '999999');
      return r;
    },
  ]) {
    let calls = 0;
    const loader = createRuntimePackageLoader(runtime, () => {}, (async () => {
      calls++;
      return factory();
    }) as typeof fetch);
    await assert.rejects(loader.load());
    assert.equal(calls, 1, 'WASM must not be fetched after a bad module');
  }
});

test('retirement and owner change during actual chunked fetch cannot publish verified assets', async () => {
  for (const retirement of ['dispose', 'owner'] as const) {
    const { runtime } = fixture();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    let allowed = true;
    let signal: AbortSignal | undefined;
    let entered!: () => void;
    const started = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const loader = createRuntimePackageLoader(
      runtime,
      () => {
        if (!allowed) throw Error('owner changed');
      },
      (async (_url, options) => {
        signal = options?.signal as AbortSignal;
        const bytes = new TextEncoder().encode(moduleText);
        const r = new Response(
          new ReadableStream({
            async start(controller) {
              controller.enqueue(bytes.subarray(0, 5));
              entered();
              await gate;
              controller.enqueue(bytes.subarray(5));
              controller.close();
            },
          }),
          { headers: { 'content-type': 'text/javascript' } },
        );
        Object.defineProperty(r, 'url', { value: runtime.module.url });
        return r;
      }) as typeof fetch,
    );
    let publications = 0;
    const loading = loader.load().then(() => {
      publications++;
    });
    await started;
    if (retirement === 'dispose') loader.dispose();
    else allowed = false;
    release();
    await assert.rejects(loading, /retired|owner changed/);
    assert.equal(publications, 0);
    if (retirement === 'dispose') assert.equal(signal?.aborted, true);
  }
});

test('trusted parent accepts only its own opaque child fixed load request and transfers binary once', async () => {
  const { runtime } = fixture();
  const { calls, fetcher } = network(runtime);
  const original = globalThis.fetch;
  globalThis.fetch = fetcher;
  const messages: unknown[][] = [];
  const child = { postMessage: (...args: unknown[]) => messages.push(args) };
  try {
    const host = createRuntimeHost(
      () => child as unknown as Window,
      runtime,
      () => {},
    );
    assert.equal(
      host.receive({ source: null, origin: 'null', data: { zoenRuntime: 1, kind: 'load' } }),
      true,
    );
    assert.equal(
      host.receive({
        source: child as unknown as Window,
        origin: 'https://other.test',
        data: { zoenRuntime: 1, kind: 'load' },
      }),
      true,
    );
    assert.equal(
      host.receive({
        source: child as unknown as Window,
        origin: 'null',
        data: { zoenRuntime: 1, kind: 'load', url: 'https://evil.test/?artifact=secret' },
      }),
      false,
    );
    assert.equal(calls.length, 0);
    const event = {
      source: child as unknown as Window,
      origin: 'null',
      data: { zoenRuntime: 1, kind: 'load' },
    };
    host.receive(event);
    host.receive(event);
    await tick();
    await tick();
    assert.equal(calls.length, 2);
    assert.equal(messages.length, 1);
    const [data, origin, transfer] = messages[0] as [{ wasm: Uint8Array }, string, ArrayBuffer[]];
    assert.equal(origin, '*');
    assert.deepEqual(data.wasm, wasm);
    assert.equal(transfer[0], data.wasm.buffer);
    host.dispose();
  } finally {
    globalThis.fetch = original;
  }
});

test('retired parent cancels a response arriving after fetch and cannot deliver it to its surviving WindowProxy', async () => {
  const { runtime } = fixture();
  const original = globalThis.fetch;
  let release!: () => void;
  let cancelled = 0;
  let fetchSignal: AbortSignal | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  globalThis.fetch = (async (_url, options) => {
    fetchSignal = options?.signal as AbortSignal;
    await gate;
    const response = new Response(
      new ReadableStream({
        cancel() {
          cancelled++;
        },
      }),
      {
        headers: { 'content-type': 'text/javascript' },
      },
    );
    Object.defineProperty(response, 'url', { value: runtime.module.url });
    return response;
  }) as typeof fetch;
  const messages: unknown[] = [];
  const child = { postMessage: (...args: unknown[]) => messages.push(args) };
  try {
    const host = createRuntimeHost(
      () => child as unknown as Window,
      runtime,
      () => {},
    );
    host.receive({
      source: child as unknown as Window,
      origin: 'null',
      data: { zoenRuntime: 1, kind: 'load' },
    });
    host.dispose();
    assert.equal(fetchSignal?.aborted, true);
    release();
    await tick();
    await tick();
    assert.equal(cancelled, 1);
    assert.deepEqual(messages, []);
  } finally {
    globalThis.fetch = original;
  }
});

test('native compiled shell actually intercepts the package channel before the unchanged JSON RPC bridge and retires it', async () => {
  const { runtime } = fixture();
  const { calls, fetcher } = network(runtime);
  const events = new Map<string, (event: unknown) => void>();
  const sent: unknown[] = [];
  const rpc: unknown[] = [];
  const child = { postMessage: (...args: unknown[]) => sent.push(args) };
  const frame = { contentWindow: child, srcdoc: '', remove() {} };
  const window = {
    addEventListener: (key: string, listener: (event: unknown) => void) =>
      events.set(key, listener),
    removeEventListener: (key: string) => events.delete(key),
    ReactNativeWebView: { postMessage: (message: string) => rpc.push(message) },
  };
  const nonce = '11111111-1111-4111-8111-111111111111';
  const html = nativeMiniappHtml('<html></html>', 'Deck', nonce, runtime);
  runInNewContext(html.match(/<script>([\s\S]*)<\/script>/)![1]!, {
    window,
    document: { getElementById: () => frame },
    fetch: fetcher,
    AbortController,
    TextEncoder,
    TextDecoder,
    Uint8Array,
    URL,
    Blob,
    console,
  });
  assert.equal(calls.length, 0);
  events.get('message')?.({
    source: child,
    origin: 'null',
    data: { zoenRuntime: 1, kind: 'load' },
  });
  await tick();
  await tick();
  assert.equal(sent.length, 1);
  assert.equal(rpc.length, 0);
  assert.equal(calls.length, 2);
  events.get('pagehide')?.({});
  assert.equal(frame.srcdoc, '');
  assert.equal(events.has('message'), false);
});

test('package opt-in remains artifact-only, CSP denies child network and unchecked remote/data modules', async () => {
  const { runtime } = fixture();
  const text = '<html><head></head><body><canvas id="deckcraft_canvas"></canvas></body></html>';
  const manifest = {
    id: 'deckcraft',
    name: 'Presentation',
    capabilities: [],
    allowedDomains: [],
    linkDomains: [],
    sha256: hash(text),
    bytes: Buffer.byteLength(text),
    runtimePackage: runtime,
  };
  const resource = {
    uri: 'ui://zoen/deckcraft/index.html',
    mime_type: 'text/html;profile=mcp-app',
    text,
    manifest_json: JSON.stringify(manifest),
    prefers_border: false,
  };
  const accepted = await verifyMiniappResource(resource, 'deckcraft');
  assert.deepEqual(accepted.manifest.runtimePackage, runtime);
  for (const change of [
    { allowedDomains: ['code.example.test'] },
    { capabilities: [{ id: 'camera', purpose: 'Photo' }] },
    { linkDomains: ['example.test'] },
  ])
    await assert.rejects(
      verifyMiniappResource(
        { ...resource, manifest_json: JSON.stringify({ ...manifest, ...change }) },
        'deckcraft',
      ),
    );
  await assert.rejects(
    verifyMiniappResource({ ...resource, uri: 'ui://other/index.html' }, 'deckcraft'),
  );
  const csp = miniappCsp([], runtime);
  assert.match(csp, /connect-src 'none';/);
  assert.match(csp, /script-src 'unsafe-inline' blob: 'wasm-unsafe-eval';/);
  assert.doesNotMatch(csp, /https:|code\.example|\*|'unsafe-eval'|script-src[^;]*data:/);
  assert.throws(() => miniappCsp(['code.example.test'], runtime));
  assert.doesNotMatch(miniappCsp([]), /wasm-unsafe-eval/);
  const secured = sandboxHtml(text, [], runtime);
  assert.ok(secured.indexOf('Content-Security-Policy') < secured.indexOf('installRuntimeClient'));
});

test('actual client module initializes exact WASM once and managed start cannot occur after retirement', async () => {
  const originalCreate = URL.createObjectURL,
    originalRevoke = URL.revokeObjectURL;
  const shared = globalThis as typeof globalThis & {
    runtimeTest?: { starts: number; retires: number; ready?: Promise<void> };
  };
  shared.runtimeTest = { starts: 0, retires: 0 };
  let revoked = 0;
  const module = `export default async({module_or_path})=>{await WebAssembly.instantiate(module_or_path);await globalThis.runtimeTest.ready;};export function zoenHostStart(){globalThis.runtimeTest.starts++};export function zoenHostRetire(){globalThis.runtimeTest.retires++}`;
  const { runtime } = fixture(module);
  URL.createObjectURL = () =>
    `data:text/javascript;base64,${Buffer.from(module).toString('base64')}`;
  URL.revokeObjectURL = () => {
    revoked++;
  };
  try {
    for (const retire of [false, true]) {
      let release!: () => void;
      shared.runtimeTest.ready = new Promise<void>((resolve) => {
        release = resolve;
      });
      const events = new Map<string, EventListener>();
      let requests = 0;
      const parent = {
        postMessage: () => {
          requests++;
        },
      };
      const target = {
        parent,
        addEventListener: (name: string, fn: EventListener) => events.set(name, fn),
        removeEventListener: (name: string) => events.delete(name),
      } as unknown as Window;
      const client = installRuntimeClient(runtime, target);
      assert.equal(requests, 0);
      const pending = client.load();
      assert.equal(client.load(), pending);
      assert.equal(requests, 1);
      const failure = retire ? assert.rejects(pending, /retired/) : undefined;
      const event = {
        source: parent,
        data: { zoenRuntime: 1, kind: 'ready', revision, module, wasm: wasm.slice() },
      } as unknown as MessageEvent;
      const receiving = events.get('message')?.(event);
      await tick();
      if (retire) client.dispose();
      release();
      await receiving;
      if (retire) await failure;
      else await pending;
      if (!retire) assert.equal(shared.runtimeTest.starts, 1);
      else assert.equal(shared.runtimeTest.starts, 1, 'retired init must not invoke managed start');
      client.dispose();
      assert.throws(() => client.load(), /retired/);
    }
    assert.equal(revoked, 2);
  } finally {
    URL.createObjectURL = originalCreate;
    URL.revokeObjectURL = originalRevoke;
    delete shared.runtimeTest;
  }
});
