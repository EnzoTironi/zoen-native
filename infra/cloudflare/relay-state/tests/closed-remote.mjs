// Opt-in remote private-service QA. No clock overrides, invite minting or public routes.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { unstable_dev } from 'wrangler';

if (process.env.ZOEN_CF_REMOTE_CLOSED_QA !== '1')
  throw new Error('Set ZOEN_CF_REMOTE_CLOSED_QA=1 to write one bounded synthetic closed Group through the private service binding.');
if (!process.env.ZOEN_CLOSED_FIXTURE)
  throw new Error('ZOEN_CLOSED_FIXTURE must identify a paired fresh native FDB fixture.');

const runnerPath = fileURLToPath(import.meta.url);
const directory = dirname(runnerPath);
const root = resolve(directory, '../../../..');
const fixturePath = resolve(process.env.ZOEN_CLOSED_FIXTURE);
let output;
const args = process.argv.slice(2);
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--out-dir' && args[i + 1] && !args[i + 1].startsWith('--')) output = resolve(args[++i]);
  else throw new Error('Usage: closed-remote.mjs [--out-dir DIR]');
}
output ??= await mkdtemp(join(tmpdir(), 'zoen-cloudflare-closed-remote-'));
await mkdir(output, { recursive: true });
const templatePath = join(directory, 'closed-remote-fixture.mjs');
const config = join(directory, 'closed.remote.wrangler.jsonc');
const previewPath = join(output, 'closed-remote-preview.mjs');
const verifierDirectory = resolve(directory, '../generated/wasm');
const scope = 'Deployed private closed Group via authenticated remote service binding; one fresh synthetic Space and a second preview client session. No public authentication/sync, invite/clock overrides, encrypted groups, or forced remote Durable Object restart acceptance.';
let phase = 'validate-paired-native-fixture';
let worker;
let result;
let failed = false;

async function artifact(path) {
  const bytes = await readFile(path);
  return { path, bytes: bytes.byteLength, sha256: createHash('sha256').update(bytes).digest('hex') };
}
async function open() {
  return unstable_dev(previewPath, {
    config, ip: '127.0.0.1', port: 0, local: false, logLevel: 'none',
    services: [{ binding: 'RELAY_CLOSED', service: 'zoen-relay-closed', remote: true }],
    experimental: { disableExperimentalWarning: true, disableDevRegistry: true, watch: false, showInteractiveDevSession: false },
  });
}
async function stop() {
  if (worker) { await worker.stop(); worker = undefined; }
}
async function call(path, input = {}) {
  const response = await worker.fetch(`http://closed-qa.test${path}`, { method: 'POST', body: JSON.stringify(input) });
  const body = response.headers.get('content-type')?.includes('application/json') ? await response.json() : null;
  return { status: response.status, body };
}
async function state() {
  const snapshot = await call('/snapshot');
  const history = await call('/read');
  assert.equal(snapshot.status, 200);
  assert.equal(history.status, 200);
  return { snapshot: snapshot.body.result, history: history.body.result };
}
function publication(value) {
  return { kind: value.kind, event: value.event, audience: value.audience, joined: value.joined };
}

try {
  const native = JSON.parse(await readFile(fixturePath, 'utf8'));
  assert.match(native.space_id, /^sp_[0-9a-hjkmnp-tv-z]{26}$/);
  const mint = native.steps.findIndex(step => step.op === 'invite');
  assert.equal(mint, 11, 'The bounded native prefix must contain exactly eleven publish operations');
  const steps = native.steps.slice(0, mint);
  assert.ok(steps.every(step => step.op === 'publish'));
  const bad = native.unsupported.find(value => value.space_id === native.space_id && value.expected_error?.code === 'bad_signature');
  assert.equal(bad?.steps.length, 1, 'A stored-CID invalid-signature fixture is required');
  const accepted = steps.filter(step => step.expected.kind === 'new');
  assert.ok(accepted.length > 0 && accepted.length <= 8);
  const canonicalBytes = accepted.reduce((sum, step) => sum + Buffer.from(step.expected.event.bytes_base64, 'base64').byteLength, 0);
  assert.ok(canonicalBytes <= 20 * 1024, 'Remote writes exceed the QA byte bound');
  const verifier = await import(pathToFileURL(join(verifierDirectory, 'admission.js')).href);
  verifier.initSync({ module: new WebAssembly.Module(await readFile(join(verifierDirectory, 'admission_bg.wasm'))) });
  const tuple = step => {
    assert.equal(typeof step.frame_base64, 'string');
    assert.ok(Buffer.from(step.frame_base64, 'base64').byteLength <= 20 * 1024);
    assert.equal(typeof step.session.authenticated_identity, 'string');
    assert.equal(typeof step.session.authenticated_device, 'string');
    if (step.target_registered !== null) assert.equal(typeof step.target_identity, 'string');
    return { frame_base64: step.frame_base64, session: step.session,
      target: step.target_registered === null ? null : { identity: step.target_identity, registered: step.target_registered } };
  };
  for (const step of steps) {
    const verified = JSON.parse(verifier.verifyPublish(Buffer.from(step.frame_base64, 'base64'), JSON.stringify(step.session)));
    assert.equal(verified.space_id, native.space_id, 'Shared Rust must bind every allowed signed frame to the one fresh Space');
    assert.equal(verified.lookup.target_identity, step.target_identity);
  }
  let invalid;
  try { verifier.verifyPublish(Buffer.from(bad.steps[0].frame_base64, 'base64'), JSON.stringify(bad.steps[0].session)); }
  catch (error) { invalid = JSON.parse(String(error)); }
  assert.deepEqual(invalid, bad.expected_error, 'The extra allowed frame must be rejected by shared Rust before it can route a write');
  const allowlist = { space_id: native.space_id, frames: [...steps, bad.steps[0]].map(tuple) };
  const template = await readFile(templatePath, 'utf8');
  const marker = 'const ALLOWED = null;';
  assert.equal(template.split(marker).length, 2, 'Preview template must have exactly one fail-closed marker');
  await writeFile(previewPath, template.replace(marker, `const ALLOWED = ${JSON.stringify(allowlist)};`));

  phase = 'open-authenticated-preview';
  worker = await open();
  phase = 'verify-fresh-empty-space';
  const empty = await state();
  assert.deepEqual(empty, { snapshot: { head: null, projection: null }, history: [] });
  phase = 'reject-caller-supplied-frame-or-space';
  assert.equal((await call('/publish', { index: 0, frame_base64: steps[0].frame_base64 })).status, 403);
  assert.equal((await call('/publish', { index: 0, spaceId: native.space_id })).status, 403);
  assert.equal((await call('/snapshot', { spaceId: native.space_id })).status, 403);

  for (let index = 0; index < steps.length; index++) {
    const step = steps[index];
    phase = `native-publish-${index}`;
    const before = await state();
    const response = await call('/publish', { index });
    if (step.expected.kind === 'rejected') {
      assert.equal(response.status, 400);
      assert.equal(response.body.error.reason, step.expected.reason);
      assert.equal(response.body.error.permanent, step.expected.permanent);
    } else {
      assert.equal(response.status, 200);
      assert.deepEqual(publication(response.body.result), step.expected);
    }
    const after = await state();
    if (step.expected.kind !== 'new') assert.deepEqual(after, before);
    assert.deepEqual(Object.entries(after.snapshot.projection?.members ?? {}).sort(([a], [b]) => a.localeCompare(b)), step.roles_after);
  }
  phase = 'reject-invalid-signature-before-stored-cid-replay';
  const beforeBad = await state();
  const rejected = await call('/publish', { index: steps.length });
  assert.equal(rejected.status, 400);
  assert.deepEqual(rejected.body.error, bad.expected_error);
  assert.deepEqual(await state(), beforeBad);

  phase = 'private-http-surface';
  const surface = await call('/surface');
  assert.equal(surface.status, 200);
  assert.ok(surface.body.result.length > 0 && surface.body.result.every(entry => entry.status === 404));
  phase = 'reopen-preview-client';
  await stop();
  worker = await open();
  assert.deepEqual(await state(), beforeBad);
  const replayIndex = steps.findIndex(step => step.expected.kind === 'duplicate');
  assert.ok(replayIndex >= 0);
  phase = 'original-replay-after-preview-client-reopen';
  const replay = await call('/publish', { index: replayIndex });
  assert.equal(replay.status, 200);
  assert.deepEqual(publication(replay.body.result), steps[replayIndex].expected);
  assert.deepEqual(await state(), beforeBad);
  assert.deepEqual(beforeBad.history.map(event => ({ seq: event.seq, prev: event.prev, hash: event.hash,
    bytes_base64: Buffer.from(Object.values(event.bytes)).toString('base64') })), accepted.map(step => step.expected.event));

  const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  const checkoutHead = git('rev-parse', 'HEAD');
  const sourceScope = ['infra/cloudflare/relay-state', 'crates/roda-relay-admission', 'crates/zoen-relay/examples/cloudflare-closed-fixture.rs'];
  result = {
    passed: true, scope, spaceId: native.space_id, nativePublishSteps: steps.length,
    acceptedEvents: accepted.length, canonicalEventBytes: canonicalBytes,
    invalidSignatureRejected: true, callerPayloadRefused: true, previewClientReopened: true,
    publicHttpSurface: surface.body.result,
    source: { commit: process.env.GITHUB_SHA ?? checkoutHead, checkoutHead,
      dirty: git('status', '--porcelain', '--', ...sourceScope).length > 0, dirtyScope: sourceScope },
    artifacts: {
      nativeFixtures: await artifact(fixturePath), runner: await artifact(runnerPath),
      bridgeTemplate: await artifact(templatePath), previewModule: await artifact(previewPath), config: await artifact(config),
      localVerifierJavascript: await artifact(join(verifierDirectory, 'admission.js')),
      localVerifierWasm: await artifact(join(verifierDirectory, 'admission_bg.wasm')),
    },
  };
} catch {
  failed = true;
  result = { passed: false, scope, phase, error: 'Remote QA did not complete. Raw Wrangler errors and authenticated preview addresses are intentionally omitted.' };
} finally {
  try { await stop(); }
  catch { failed = true; result = { ...result, passed: false, cleanupFailed: true }; }
}
await writeFile(join(output, 'summary.json'), `${JSON.stringify(result, null, 2)}\n`);
console.log(JSON.stringify({ passed: result.passed, output, phase: failed ? phase : 'complete', scope }));
if (failed) process.exitCode = 1;
