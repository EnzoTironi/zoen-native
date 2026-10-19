#!/usr/bin/env node
// Shared native/Node/workerd catch-up planning, without a relay or deployment.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFile, mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
let skipBuild = false;
let requestedOutput;
for (let index = 0; index < args.length; index++) {
  if (args[index] === '--skip-build' && !skipBuild) skipBuild = true;
  else if (args[index] === '--out-dir' && !requestedOutput && args[index + 1] && !args[index + 1].startsWith('--'))
    requestedOutput = args[++index];
  else throw new Error('Usage: verify-cloudflare-sync.mjs [--skip-build] [--out-dir DIR]');
}
const output = requestedOutput ? resolve(requestedOutput) : await mkdtemp(join(tmpdir(), 'zoen-cloudflare-sync-'));
if (requestedOutput) {
  await mkdir(output, { recursive: true });
  assert.equal((await readdir(output)).length, 0, 'Preserve existing sync evidence: output directory must be empty');
}
const fixturePath = join(output, 'native-fixture.json');
const wasmDirectory = join(output, 'wasm');
console.log(`Catch-up planning evidence: ${output}`);
function run(command, commandArgs, capture = false, env = process.env) {
  return new Promise((accept, reject) => {
    const child = spawn(command, commandArgs, { cwd: root, env, stdio: ['ignore', capture ? 'pipe' : 'inherit', 'inherit'] });
    const chunks = [];
    if (capture) child.stdout.on('data', bytes => chunks.push(bytes));
    child.once('error', reject);
    child.once('close', (code, signal) => code === 0
      ? accept(capture ? Buffer.concat(chunks) : undefined)
      : reject(new Error(`${command} failed (${signal ?? code})`)));
  });
}
if (!skipBuild)
  await run(process.execPath, [join(root, 'scripts/build-cloudflare-closed.mjs')]);
await mkdir(wasmDirectory);
for (const file of ['admission.js', 'admission_bg.wasm'])
  await copyFile(join(root, 'infra/cloudflare/relay-state/generated/wasm', file), join(wasmDirectory, file));
await writeFile(join(wasmDirectory, 'package.json'), '{"type":"module"}\n', { flag: 'wx' });
const fixture = await run('cargo', ['run', '--quiet', '--locked', '-p', 'roda-relay-admission', '--example', 'sync-fixture'], true);
await writeFile(fixturePath, fixture, { flag: 'wx' });
assert.equal(JSON.parse(fixture).steps.length, 8);
await run(process.execPath, ['--test', join(root, 'infra/cloudflare/relay-state/tests/sync-plan.test.mjs')], false,
  { ...process.env, ZOEN_SYNC_FIXTURE: fixturePath, ZOEN_SYNC_WASM_DIR: wasmDirectory });
const artifacts = {};
for (const [name, path] of [
  ['nativeFixture', fixturePath],
  ['javascript', join(wasmDirectory, 'admission.js')],
  ['wasm', join(wasmDirectory, 'admission_bg.wasm')],
]) {
  const bytes = await readFile(path);
  artifacts[name] = { path, bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
}
const git = (...commandArgs) => execFileSync('git', commandArgs, { cwd: root, encoding: 'utf8' }).trim();
const result = {
  schema: 1,
  result: 'passed',
  scope: 'Shared catch-up visibility only: 8 native protobuf vectors agree in Node WASM and local workerd. Device authentication, membership storage/indexing, paginated reads, delivery fencing, public sync and runtime WebSocket closure remain separate gates.',
  source: { checkoutHead: git('rev-parse', 'HEAD'), dirty: git('status', '--porcelain', '--',
    'crates/roda-relay-admission', 'crates/zoen-relay/src/session.rs',
    'infra/cloudflare/relay-state/tests/sync-plan.test.mjs', 'scripts/verify-cloudflare-sync.mjs').length > 0 },
  nativeVectors: 8,
  nodeTests: 1,
  workerdTests: 1,
  artifacts,
};
await writeFile(join(output, 'acceptance.json'), `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
console.log(JSON.stringify(result, null, 2));
