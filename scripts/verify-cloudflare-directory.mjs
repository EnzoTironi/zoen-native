#!/usr/bin/env node
// Isolated native PostgreSQL oracle versus a private Rust/WASM Directory DO.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFile, mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
let output;
let skipBuild = false;
const args = process.argv.slice(2);
for (let index = 0; index < args.length; index++) {
  if (args[index] === '--skip-build') skipBuild = true;
  else if (args[index] === '--out-dir' && args[index + 1] && !args[index + 1].startsWith('--')) output = resolve(args[++index]);
  else throw new Error('Usage: verify-cloudflare-directory.mjs [--out-dir DIR] [--skip-build]');
}
output ??= await mkdtemp(join(tmpdir(), 'zoen-cloudflare-directory-'));
await mkdir(output, { recursive: true });
const environment = { ...process.env, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? '2' };
const scope = 'Private single-cell Person/device directory, Directory-owned native binary sockets and MLS key-package publication/claims (128 records/768 KiB response bound). Native PostgreSQL policy versus shared Rust/WASM and SQLite transactions; no public endpoint, Space audience authorization, provider auth, recovery or agent registration.';
const sourcePaths = [
  'Cargo.toml', 'Cargo.lock', 'crates/roda-relay-admission/Cargo.toml',
  'crates/roda-relay-admission/src/auth.rs',
  'crates/roda-relay-admission/src/auth/fixtures/rsa-test-only.jwk.json',
  'crates/roda-relay-admission/src/auth/fixtures/rsa-test-only.pk8',
  'crates/roda-relay-admission/src/directory.rs',
  'crates/roda-relay-admission/src/directory/key_packages.rs',
  'crates/roda-relay-admission/src/directory/socket.rs',
  'crates/roda-relay-admission/tests/directory_socket.rs',
  'crates/roda-relay-admission/tests/directory_key_packages.rs',
  'crates/roda-relay-admission/tests/directory.rs', 'crates/roda-relay-admission/src/lib.rs',
  'crates/zoen-relay/src/db.rs', 'crates/zoen-relay/src/session.rs', 'crates/zoen-relay/src/key_package_claims.rs',
  'crates/zoen-relay/examples/cloudflare-directory-fixture.rs',
  'infra/cloudflare/relay-state/src/directory-state.mjs', 'infra/cloudflare/relay-state/src/directory-index.mjs',
  'infra/cloudflare/relay-state/tests/directory-fixture.mjs', 'infra/cloudflare/relay-state/tests/directory-state.test.mjs',
  'infra/cloudflare/relay-state/tests/directory-socket.test.mjs',
  'infra/cloudflare/relay-state/wrangler.directory.jsonc', 'infra/cloudflare/relay-state/package.json',
  'infra/cloudflare/relay-state/DIRECTORY.md', 'scripts/verify-cloudflare-directory.mjs',
  'scripts/verify-cloudflare-admission.mjs', 'scripts/build-cloudflare-closed.mjs',
  'infra/cloudflare/package.json', 'infra/cloudflare/package-lock.json',
];
function run(command, commandArgs, { capture = false, env = environment } = {}) {
  console.log(`Running ${command} ${commandArgs.join(' ')}`);
  return new Promise((accept, reject) => {
    const child = spawn(command, commandArgs, { cwd: root, env, stdio: ['ignore', capture ? 'pipe' : 'inherit', 'inherit'] });
    const chunks = [];
    if (capture) child.stdout.on('data', chunk => chunks.push(chunk));
    child.once('error', reject);
    child.once('close', (code, signal) => {
      const stdout = capture ? Buffer.concat(chunks).toString('utf8') : undefined;
      if (code === 0) accept(stdout);
      else reject(Object.assign(new Error(`${command} failed (${signal ?? code})`), { stdout }));
    });
  });
}
function testTotals(tap) {
  const totals = {};
  for (const name of ['tests', 'pass', 'fail', 'cancelled', 'skipped', 'todo']) {
    const matches = [...tap.matchAll(new RegExp(`^# ${name} (\\d+)\\r?$`, 'gm'))];
    assert.ok(matches.length, `Missing Node TAP ${name} total`);
    totals[name] = Number(matches.at(-1)[1]);
  }
  assert.ok(totals.tests > 0, 'The workerd suite must run actual cases');
  assert.equal(totals.pass, totals.tests, 'Every workerd case must pass');
  for (const name of ['fail', 'cancelled', 'skipped', 'todo']) assert.equal(totals[name], 0, `Unexpected workerd ${name}`);
  return totals;
}
async function artifact(path) {
  const bytes = await readFile(path);
  return { path, bytes: bytes.byteLength, sha256: createHash('sha256').update(bytes).digest('hex') };
}
async function summary(value) { await writeFile(join(output, 'summary.json'), `${JSON.stringify(value, null, 2)}\n`); }
try {
  if (!skipBuild) await run(process.execPath, [join(root, 'scripts/build-cloudflare-closed.mjs')]);
  const database = environment.ZOEN_TEST_PG ?? environment.DATABASE_URL;
  if (!database) throw new Error('Set ZOEN_TEST_PG or DATABASE_URL to an existing PostgreSQL test database');
  const fixture = JSON.parse(await run('cargo', ['run', '--quiet', '--locked', '-p', 'zoen-relay', '--example', 'cloudflare-directory-fixture'], {
    capture: true, env: { ...environment, ZOEN_TEST_PG: database },
  }));
  assert.equal(fixture.schema, 1);
  assert.equal(fixture.steps.length, 71, 'Every native directory and key-package scenario must run');
  const fixturePath = join(output, 'directory-native-fixtures.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
  const workerdTapPath = join(output, 'directory-workerd.tap');
  const workerdTap = await run(process.execPath, ['--test', '--test-reporter=tap', '--test-concurrency=1', 'infra/cloudflare/relay-state/tests/directory-state.test.mjs', 'infra/cloudflare/relay-state/tests/directory-socket.test.mjs'], {
    capture: true, env: { ...environment, ZOEN_DIRECTORY_FIXTURE: fixturePath },
  }).catch(async failure => {
    if (typeof failure.stdout === 'string') {
      await writeFile(workerdTapPath, failure.stdout);
      process.stdout.write(failure.stdout);
    }
    throw failure;
  });
  await writeFile(workerdTapPath, workerdTap);
  process.stdout.write(workerdTap);
  const workerd = testTotals(workerdTap);
  const copied = join(output, 'wasm');
  await mkdir(copied, { recursive: true });
  for (const name of ['admission.js', 'admission_bg.wasm']) await copyFile(join(root, 'infra/cloudflare/relay-state/generated/wasm', name), join(copied, name));
  const checkoutHead = (await run('git', ['rev-parse', 'HEAD'], { capture: true })).trim();
  const dirty = Boolean((await run('git', ['status', '--porcelain', '--', ...sourcePaths], { capture: true })).trim());
  await summary({ passed: true, scope, output, skipBuild, nativeSteps: fixture.steps.length, workerdCases: workerd.tests, workerd,
    source: { commit: process.env.GITHUB_SHA ?? checkoutHead, checkoutHead, dirty, dirtyScope: sourcePaths },
    sources: await Promise.all(sourcePaths.map(path => artifact(join(root, path)))),
    artifacts: await Promise.all([fixturePath, workerdTapPath, join(copied, 'admission.js'), join(copied, 'admission_bg.wasm')].map(artifact)),
  });
  console.log(`Private directory verification passed; receipt ${join(output, 'summary.json')}`);
} catch (failure) {
  await summary({ passed: false, scope, output, skipBuild, error: failure.message });
  throw failure;
}
