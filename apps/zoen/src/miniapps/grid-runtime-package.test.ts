import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { parseRuntimePackage } from './runtime-package.ts';
import { miniappCsp, verifyMiniappResource } from './sandbox.ts';
const hash = (bytes: Uint8Array | string) => createHash('sha256').update(bytes).digest('hex');
function fixture() {
  const revision = 'a'.repeat(40);
  const root = `https://code.example.test/deckcraft/${revision}/`;
  const module =
    'export default async()=>{};export function zoenHostStart(){};export function zoenHostRetire(){}';
  const wasm = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);
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
  return { runtime };
}

test('the fixed Grid runtime reuses exact asset limits and retains its own engine identity', () => {
  const { runtime } = fixture();
  const grid = parseRuntimePackage({ ...runtime, engine: 'gridcraft' });
  assert.equal(grid.engine, 'gridcraft');
  assert.equal(grid.abi, runtime.abi);
  assert.deepEqual(grid.module, runtime.module);
  assert.deepEqual(grid.wasm, runtime.wasm);
  for (const engine of ['GridCraft', 'other', '', null, {}, ['gridcraft']])
    assert.throws(() => parseRuntimePackage({ ...runtime, engine }));
  assert.throws(() =>
    parseRuntimePackage({ ...grid, wasm: { ...grid.wasm, bytes: 48 * 1024 * 1024 + 1 } }),
  );
});

test('Grid artifact runtime is bound to the fixed App and URI, with no new permissions', async () => {
  const { runtime } = fixture();
  const grid = parseRuntimePackage({ ...runtime, engine: 'gridcraft' });
  const text = '<html><head></head><body><canvas id="gridcraft_canvas"></canvas></body></html>';
  const manifest = {
    id: 'gridcraft',
    name: 'Workbook',
    capabilities: [],
    allowedDomains: [],
    linkDomains: [],
    sha256: hash(text),
    bytes: Buffer.byteLength(text),
    runtimePackage: grid,
  };
  const resource = {
    uri: 'ui://zoen/gridcraft/index.html',
    mime_type: 'text/html;profile=mcp-app',
    text,
    manifest_json: JSON.stringify(manifest),
    prefers_border: false,
  };
  assert.equal(
    (await verifyMiniappResource(resource, 'gridcraft')).manifest.runtimePackage?.engine,
    'gridcraft',
  );
  for (const change of [
    { runtimePackage: runtime },
    { capabilities: [{ id: 'camera', purpose: 'Photo' }] },
    { allowedDomains: ['example.test'] },
    { linkDomains: ['example.test'] },
  ])
    await assert.rejects(
      verifyMiniappResource(
        { ...resource, manifest_json: JSON.stringify({ ...manifest, ...change }) },
        'gridcraft',
      ),
    );
  await assert.rejects(
    verifyMiniappResource({ ...resource, uri: 'ui://zoen/deckcraft/index.html' }, 'gridcraft'),
  );
  await assert.rejects(
    verifyMiniappResource(
      { ...resource, manifest_json: JSON.stringify({ ...manifest, id: 'deckcraft' }) },
      'deckcraft',
    ),
  );
  assert.match(miniappCsp([], grid), /connect-src 'none';/);
});
