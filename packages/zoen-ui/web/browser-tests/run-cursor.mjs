// Opt-in source-qualified fixture only. Expects an existing wasm-bindgen output;
// never downloads, builds Cargo, starts a retained service, or opens a user profile.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createHash, createPrivateKey, createPublicKey } from 'node:crypto';
import { readFile, writeFile, mkdir, stat } from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { findBrowserExecutable } from '../../../../miniapps/excalidraw-ui/test/browser-driver.mjs';
import { syntheticRelay } from './cursor-relay.mjs';
import { runBrowser } from './cursor-browser.mjs';
const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
assert.equal(args.length, 6, 'Use --fixture FILE --generated-root DIR --output NEWDIR');
const flags = Object.fromEntries(
  Array.from({ length: 3 }, (_, i) => [args[i * 2], args[i * 2 + 1]]),
);
assert.deepEqual(Object.keys(flags).sort(), ['--fixture', '--generated-root', '--output']);
const output = resolve(flags['--output']),
  generated = resolve(flags['--generated-root']);
await mkdir(output, { mode: 0o700 }); // Refuse reuse: every attempt retains its own outcome.
const fixtureBytes = await readFile(resolve(flags['--fixture']));
assert.ok(fixtureBytes.length <= 16 * 1_048_576);
const fixture = JSON.parse(fixtureBytes);
assert.equal(fixture.format, 'zoen-synthetic-browser-cursor-fixture-v1');
function expectedPublic(byte) {
  const key = createPrivateKey({
    key: Buffer.concat([
      Buffer.from('302e020100300506032b657004220420', 'hex'),
      Buffer.alloc(32, byte),
    ]),
    format: 'der',
    type: 'pkcs8',
  });
  return createPublicKey(key).export({ format: 'der', type: 'spki' }).subarray(-32).toString('hex');
}
assert.equal(
  fixture.account.device,
  expectedPublic(113),
  'Only the fixed synthetic signer is accepted',
);
assert.equal(
  fixture.account.identity,
  expectedPublic(111),
  'Only the fixed synthetic owner is accepted',
);
const chrome = await findBrowserExecutable();
assert.ok(chrome, 'A real installed Chrome is required');
const wasmPath = join(generated, 'zoen_web_core_bg.wasm');
assert.ok((await stat(wasmPath)).size <= 256 * 1_048_576);
const wasm = await readFile(wasmPath),
  sri = (bytes) => `sha384-${createHash('sha384').update(bytes).digest('base64')}`;
const workerPath = join(output, 'worker.js');
await build({
  entryPoints: [join(here, '../worker.ts')],
  outfile: workerPath,
  bundle: true,
  format: 'esm',
  platform: 'browser',
  target: ['es2022'],
  define: { ZOEN_WASM_INTEGRITY: JSON.stringify(sri(wasm)) },
  plugins: [
    {
      name: 'source-qualified-core',
      setup(plugin) {
        plugin.onResolve({ filter: /\/core\/zoen_web_core\.js$/ }, () => ({
          path: join(generated, 'zoen_web_core.js'),
        }));
      },
    },
  ],
});
const worker = await readFile(workerPath),
  pagePath = join(output, 'main.js');
await build({
  entryPoints: [join(here, 'cursor-page.ts')],
  outfile: pagePath,
  bundle: true,
  format: 'esm',
  platform: 'browser',
  target: ['es2022'],
  define: { FIXTURE_WORKER_INTEGRITY: JSON.stringify(sri(worker)) },
});
const page = await readFile(pagePath),
  failures = [];
const resources = new Map([
  [
    '/',
    [
      'text/html',
      Buffer.from(
        '<!doctype html><meta charset="utf-8"><title>Owned browser cursor fixture</title><script type="module" src="/main.js"></script>',
      ),
    ],
  ],
  ['/main.js', ['text/javascript', page]],
  ['/worker.js', ['text/javascript', worker]],
  ['/core/zoen_web_core_bg.wasm', ['application/wasm', wasm]],
  ['/fixture.json', ['application/json', fixtureBytes]],
]);
let relay;
const server = createServer((request, response) => {
  response.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  response.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
  response.setHeader('Cache-Control', 'no-store');
  if (request.method !== 'GET') {
    response.writeHead(405).end();
    return;
  }
  if (request.url === '/metrics') {
    response.setHeader('Content-Type', 'application/json');
    response.end(JSON.stringify(relay.metrics));
    return;
  }
  const resource = resources.get(request.url);
  if (!resource) {
    response.writeHead(404).end();
    return;
  }
  response.setHeader('Content-Type', resource[0]);
  response.end(resource[1]);
});
relay = syntheticRelay(server, fixture, failures);
let result, failure;
const start = performance.now();
try {
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  result = await runBrowser(chrome, output, `http://127.0.0.1:${address.port}/`);
  assert.deepEqual(failures, []);
  assert.equal(result.result.status, 'passed');
} catch (error) {
  failure = error;
} finally {
  relay.close();
  await new Promise((resolveClose) => {
    server.close(resolveClose);
    server.closeAllConnections();
  });
}
const receipt = {
  format: 'zoen-owned-browser-cursor-proof-v1',
  status: failure ? 'failed' : 'passed',
  seconds: Math.round((performance.now() - start) * 1000) / 1000,
  browser: chrome,
  fixtureSha256: createHash('sha256').update(fixtureBytes).digest('hex'),
  wasmSha256: createHash('sha256').update(wasm).digest('hex'),
  workerSha256: createHash('sha256').update(worker).digest('hex'),
  pageSha256: createHash('sha256').update(page).digest('hex'),
  result,
  peerFailures: failures,
  metrics: relay.metrics,
  limitations:
    'Owned synthetic protocol peer, fixed test-only keys/SQLite MLS. Real browser Worker/WASM/WebSocket/BrowserVault; no production relay, remote revocation, provider or retained account proof.',
};
await writeFile(join(output, 'result.json'), JSON.stringify(receipt, null, 2) + '\n', {
  flag: 'wx',
  mode: 0o600,
});
console.log(
  JSON.stringify({
    status: receipt.status,
    result: result?.result,
    metrics: relay.metrics,
    seconds: receipt.seconds,
  }),
);
if (failure)
  throw Error('Owned browser cursor contract failed; see public result.json', { cause: failure });
