import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { build } from 'esbuild';
import { verifyMiniappResource } from './sandbox.ts';
import type { readVerifiedMiniappResource } from './resource-load.ts';
import type { frameAuthority } from './lifetime.ts';
import type { AppResourceDto } from '../core/generated/AppResourceDto';

const MAX_BYTES = 8 * 1024 * 1024;
function resource(text = '<html><head></head><body>Árvore 🪴\n</body></html>'): AppResourceDto {
  const bytes = new TextEncoder().encode(text);
  return {
    uri: 'ui://fixture/drawing',
    mime_type: 'text/html;profile=mcp-app',
    text,
    prefers_border: false,
    manifest_json: JSON.stringify({
      id: 'drawing',
      name: 'Drawing',
      capabilities: [{ id: 'camera', purpose: 'Add a selected photo' }],
      allowedDomains: ['tiles.example.com'],
      linkDomains: ['docs.example.com'],
      sha256: createHash('sha256').update(bytes).digest('hex'),
      bytes: bytes.length,
    }),
  };
}
const nativePort = `
  import {createHash} from 'node:crypto';
  export const calls=[];
  let gate=null;
  export function holdHash(){
    let release, entered;
    const wait = new Promise(resolve=>{release=resolve});
    const started = new Promise(resolve=>{entered=resolve});
    gate={wait,entered};
    return {release,entered:started};
  }
  export const CryptoDigestAlgorithm={SHA256:'SHA-256'};
  export const CryptoEncoding={HEX:'hex'};
  export async function digestStringAsync(algorithm,text,options){
    calls.push({algorithm,text,options});
    if(gate){const current=gate;gate=null;current.entered();await current.wait;}
    return createHash('sha256').update(text,'utf8').digest('hex');
  }
  export function digest(){throw Error('Synchronous native digest is forbidden');}
`;
const compiled = await build({
  stdin: {
    contents: `export {verifyMiniappResource} from './sandbox.ts';
      export {readVerifiedMiniappResource} from './resource-load.ts';
      export {frameAuthority} from './lifetime.ts';
      export {calls,holdHash} from 'expo-crypto';`,
    resolveDir: new URL('.', import.meta.url).pathname,
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
  define: { 'process.env.EXPO_OS': '"android"' },
  plugins: [
    {
      name: 'native-crypto-port',
      setup(builder) {
        builder.onResolve({ filter: /^(expo-crypto|\.\.\/i18n\/index\.ts)$/ }, ({ path }) => ({
          path,
          namespace: 'native-port',
        }));
        builder.onLoad({ filter: /.*/, namespace: 'native-port' }, ({ path }) => ({
          contents: path === 'expo-crypto' ? nativePort : 'export const t=key=>key;',
        }));
      },
    },
  ],
});
const native = (await import(
  `data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0]!.text).toString('base64')}`
)) as {
  verifyMiniappResource: typeof verifyMiniappResource;
  readVerifiedMiniappResource: typeof readVerifiedMiniappResource;
  frameAuthority: typeof frameAuthority;
  calls: { algorithm: string; text: string; options: { encoding: string } }[];
  holdHash(): { release(): void; entered: Promise<void> };
};

for (const [label, text] of [
  ['valid Unicode and exact newlines', '\uFEFFé e\u0301 🪴\r\n\u2028\u2029\u0000終'],
  ['isolated high surrogate', 'before\uD800after'],
  ['isolated low surrogate', 'before\uDC00after'],
  ['adjacent malformed and valid pairs', '\uD800\uD800\uDC00\uDC00\uDFFF\uDBFF'],
] as const)
  test(`native and WebCrypto preserve TextEncoder hash/byte semantics: ${label}`, async () => {
    const input = resource(text);
    const expected = new TextDecoder('utf-8', { ignoreBOM: true }).decode(
      new TextEncoder().encode(text),
    );
    const browser = await verifyMiniappResource(input, 'drawing');
    const android = await native.verifyMiniappResource(input, 'drawing');
    assert.deepEqual(android, browser);
    assert.equal(android.resource.text, expected);
    if (label === 'valid Unicode and exact newlines') assert.equal(android.resource.text, text);
    assert.equal(android.manifest.bytes, new TextEncoder().encode(text).length);
    assert.equal(
      android.manifest.sha256,
      createHash('sha256').update(new TextEncoder().encode(text)).digest('hex'),
    );
    assert.deepEqual(native.calls.at(-1), {
      algorithm: 'SHA-256',
      text: expected,
      options: { encoding: 'hex' },
    });
  });

test('resource and permission metadata are pinned and deeply immutable across the real async hash boundary', async () => {
  const input = resource();
  const original = { ...input };
  const gate = native.holdHash();
  const verifying = native.verifyMiniappResource(input, 'drawing');
  await gate.entered;
  input.text = '<script>changed while native worker was running</script>';
  input.uri = 'ui://other';
  input.manifest_json = JSON.stringify({
    capabilities: [{ id: 'microphone', purpose: 'changed' }],
  });
  gate.release();
  const accepted = await verifying;
  assert.deepEqual(accepted.resource, original);
  assert.deepEqual(accepted.manifest.capabilities, [
    { id: 'camera', purpose: 'Add a selected photo' },
  ]);
  for (const value of [
    accepted,
    accepted.resource,
    accepted.manifest,
    accepted.manifest.capabilities,
    accepted.manifest.capabilities[0],
    accepted.manifest.allowedDomains,
    accepted.manifest.linkDomains,
  ])
    assert.equal(Object.isFrozen(value), true);
  assert.equal(Reflect.set(accepted.resource, 'text', 'tampered'), false);
  assert.equal(Reflect.set(accepted.manifest.allowedDomains, '0', 'evil.test'), false);
});

test('byte-limit and declared policy failures reject before dispatching the native hash', async () => {
  const start = native.calls.length;
  const oversized = resource('é'.repeat(MAX_BYTES / 2 + 1));
  await assert.rejects(native.verifyMiniappResource(oversized, 'drawing'));
  assert.equal(native.calls.length, start);
  for (const change of [
    { id: 'another' },
    { bytes: 1 },
    { allowedDomains: ['*.example.com'] },
    { allowedDomains: ['example.com; script-src *'] },
    { linkDomains: ['https://docs.example.com'] },
    { capabilities: [{ id: 'camera', purpose: '' }] },
    {
      capabilities: [
        { id: 'camera', purpose: 'first' },
        { id: 'camera', purpose: 'second' },
      ],
    },
  ]) {
    const input = resource();
    input.manifest_json = JSON.stringify({ ...JSON.parse(input.manifest_json), ...change });
    await assert.rejects(native.verifyMiniappResource(input, 'drawing'));
  }
  assert.equal(native.calls.length, start);
  const exact = resource('a'.repeat(MAX_BYTES));
  assert.equal((await native.verifyMiniappResource(exact, 'drawing')).manifest.bytes, MAX_BYTES);
});

test('same-byte-length tampering and a forged manifest digest never yield a verified resource', async () => {
  const input = resource('original');
  await assert.rejects(native.verifyMiniappResource({ ...input, text: 'changed!' }, 'drawing'));
  const manifest = JSON.parse(input.manifest_json);
  manifest.sha256 = '00'.repeat(32);
  await assert.rejects(
    native.verifyMiniappResource({ ...input, manifest_json: JSON.stringify(manifest) }, 'drawing'),
  );
});

test('actual load orchestration suppresses a closed focus after native hashing starts', async () => {
  const lifetime = native.frameAuthority(() => {});
  const gate = native.holdHash();
  let publications = 0;
  const loading = native
    .readVerifiedMiniappResource(
      async () => resource(),
      'drawing',
      'ui://fixture/drawing',
      lifetime.check,
    )
    .then((value) => {
      publications++;
      return value;
    });
  await gate.entered;
  lifetime.close();
  gate.release();
  await assert.rejects(loading);
  assert.equal(publications, 0);
});

test('actual load orchestration rechecks identity/device/epoch after hashing and never caches another owner authority', async () => {
  let owner = 'original-device';
  const lifetime = native.frameAuthority(() => {
    if (owner !== 'original-device') throw Error('account changed');
  });
  const gate = native.holdHash();
  let publications = 0;
  const loading = native
    .readVerifiedMiniappResource(
      async () => resource(),
      'drawing',
      'ui://fixture/drawing',
      lifetime.check,
    )
    .then((value) => {
      publications++;
      return value;
    });
  await gate.entered;
  owner = 'replacement-device';
  gate.release();
  await assert.rejects(loading, /account changed/);
  assert.equal(publications, 0);
  const before = native.calls.length;
  const next = native.frameAuthority(() => {
    assert.equal(owner, 'replacement-device');
  });
  const accepted = await native.readVerifiedMiniappResource(
    async () => resource(),
    'drawing',
    'ui://fixture/drawing',
    next.check,
  );
  assert.equal(accepted.manifest.id, 'drawing');
  assert.equal(
    native.calls.length,
    before + 1,
    'a new owner receives a newly checked immutable pair',
  );
});

test('a resource arriving after focus disposal or for a different URI cannot start a hash', async () => {
  let resolve!: (value: AppResourceDto) => void;
  const response = new Promise<AppResourceDto>((done) => {
    resolve = done;
  });
  const lifetime = native.frameAuthority(() => {});
  const before = native.calls.length;
  const loading = native.readVerifiedMiniappResource(
    () => response,
    'drawing',
    'ui://fixture/drawing',
    lifetime.check,
  );
  lifetime.close();
  resolve(resource());
  await assert.rejects(loading);
  await assert.rejects(
    native.readVerifiedMiniappResource(
      async () => resource(),
      'drawing',
      'ui://another',
      () => {},
    ),
  );
  assert.equal(native.calls.length, before);
});
