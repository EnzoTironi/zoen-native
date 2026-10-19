import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import test from 'node:test';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';

const moduleRoot = fileURLToPath(new URL('../', import.meta.url));
const wasmDirectory = process.env.ZOEN_SYNC_WASM_DIR
  ? resolve(process.env.ZOEN_SYNC_WASM_DIR) : join(moduleRoot, 'generated/wasm');
const { initSync, planSync } = await import(pathToFileURL(join(wasmDirectory, 'admission.js')));
const fixture = JSON.parse(await readFile(process.env.ZOEN_SYNC_FIXTURE
  ? resolve(process.env.ZOEN_SYNC_FIXTURE) : join(moduleRoot, 'generated/sync-fixture.json'), 'utf8'));
assert.equal(fixture.schema, 1);
assert.equal(fixture.steps.length, 8);
const wasmPath = join(wasmDirectory, 'admission_bg.wasm');
initSync({ module: new WebAssembly.Module(await readFile(wasmPath)) });
const bytes = step => Uint8Array.from(Buffer.from(step.frame_base64, 'base64'));

test('Node WASM preserves native catch-up plans, removal boundaries and exact UInt64 values', () => {
  for (const step of fixture.steps)
    assert.deepEqual(JSON.parse(planSync(bytes(step), JSON.stringify(step.context))), step.expected, step.label);
});

test('local workerd preserves every native plan and refuses another identity membership snapshot', async () => {
  const mf = new Miniflare(convertV4MiniflareOptions({
    compatibilityDate: '2026-10-18',
    modulesRoot: wasmDirectory,
    modules: [
      { type: 'ESModule', path: join(wasmDirectory, 'sync-worker.mjs'), contents: `
        import { initSync, planSync } from './admission.js';
        import wasmModule from './admission_bg.wasm';
        initSync({ module: wasmModule });
        export default { async fetch(request) {
          const step = await request.json();
          try {
            const frame = Uint8Array.from(atob(step.frame_base64), char => char.charCodeAt(0));
            return Response.json({ plan: JSON.parse(planSync(frame, JSON.stringify(step.context))) });
          } catch (error) { return Response.json({ error: JSON.parse(String(error)) }, { status: 400 }); }
        } };
      ` },
      { type: 'ESModule', path: join(wasmDirectory, 'admission.js') },
      { type: 'CompiledWasm', path: wasmPath },
    ],
  }));
  try {
    for (const step of fixture.steps) {
      const response = await mf.dispatchFetch('http://sync.test', { method: 'POST', body: JSON.stringify(step) });
      assert.equal(response.status, 200, step.label);
      assert.deepEqual((await response.json()).plan, step.expected, step.label);
    }
    const wrongIdentity = structuredClone(fixture.steps[0]);
    wrongIdentity.context.facts.identity = '33'.repeat(32);
    const response = await mf.dispatchFetch('http://sync.test', { method: 'POST', body: JSON.stringify(wrongIdentity) });
    assert.equal(response.status, 400);
    const failure = (await response.json()).error;
    assert.equal(failure.code, 'invalid_sync_context');
    assert.equal(failure.permanent, false);
  } finally { await mf.dispose(); }
});
