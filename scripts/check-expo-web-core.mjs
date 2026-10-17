#!/usr/bin/env node
// Exercise the actual generated WASM and pinned worker, using disposable Ed25519
// fixtures. Production signing remains nonexportable WebCrypto in BrowserVault.
import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, sign } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const app = join(root, 'apps/zoen');
const generated = join(app, 'modules/zoen-core/generated/web');
const wasm = await readFile(join(app, 'public/core/zoen_web_core_bg.wasm'));
const worker = await readFile(join(app, 'public/worker.js'), 'utf8');
const assets = JSON.parse(await readFile(join(app, 'src/core/generated/web-assets.json'), 'utf8'));
const integrity = (bytes) => `sha384-${createHash('sha384').update(bytes).digest('base64')}`;
assert.equal(integrity(worker), assets.workerIntegrity, 'worker integrity is stale');
assert.ok(worker.includes(integrity(wasm)), 'worker points at a different WASM payload');

const core = await import(pathToFileURL(join(generated, 'zoen_web_core.js')).href);
await core.default({ module_or_path: wasm });
const identity = generateKeyPairSync('ed25519');
const device = generateKeyPairSync('ed25519');
const publicHex = (pair) => pair.publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('hex');
const deviceId = publicHex(device);
const account = {
  identity: publicHex(identity), device: deviceId,
  cert: sign(null, core.device_cert_message(deviceId), identity.privateKey).toString('hex'),
  name: 'WASM fixture', handle: 'wasm-fixture', relay: 'http://127.0.0.1:9000',
  primary: publicHex(identity), primary_hpke: '', hpke_secret: '', link_secret: '',
  agreement_secret: '', profile_key: '', profile_key_version: 0,
  profile_seen_version: 0, profile_fields: '',
};
globalThis.zoenSign = (payload) => new Uint8Array(sign(null, payload, device.privateKey));
let client;
let restored;
try {
  client = new core.BrowserCore(JSON.stringify(account), '');
  const command = (value) => JSON.parse(client.document_request(JSON.stringify(value), 1000)).result;
  const blocks = command({ op: 'page_parse_markdown', markdown: '# Viagem **junta** 🪴\n\n- [x] Conferir *amanhã*\n' });
  assert.equal(blocks.length, 2);
  assert.equal(blocks[0].kind, 'heading');
  assert.ok(blocks[0].spans.some((span) => span.key === 'b'));
  assert.equal(blocks[1].kind, 'task');
  assert.equal(blocks[1].checked, true);
  for (const block of blocks) {
    const markdown = command({ op: 'page_block_markdown', block });
    const decoded = command({ op: 'page_parse_markdown', markdown });
    assert.equal(decoded.length, 1);
    assert.deepEqual({ ...decoded[0], id: block.id }, block);
  }
  const apps = JSON.parse(client.app_request('{"op":"app_specs"}', 1000, 'pt-BR')).result;
  assert.ok(apps.some((app) => app.id === 'schedule'));
  assert.ok(apps.some((app) => app.id === 'pet'));
  const agents = JSON.parse(client.agent_request('{"op":"agents"}', 1000)).result;
  assert.deepEqual(agents, []);
  const update = JSON.parse(client.bootstrap());
  assert.ok(update.frames.length > 0);
  restored = new core.BrowserCore(JSON.stringify(account), update.snapshot);
  assert.deepEqual(JSON.parse(restored.view()), []);
  process.stdout.write(`WASM verified: ${blocks.length} rich-text blocks, ${apps.length} shared mini-apps, signed bootstrap/reload, pinned worker (${wasm.byteLength} bytes).\n`);
} finally {
  restored?.free();
  client?.free();
  delete globalThis.zoenSign;
}
