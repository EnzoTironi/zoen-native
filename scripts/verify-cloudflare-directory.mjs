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
const scope = 'Private single-cell Person/device directory only. Native PostgreSQL policy versus shared Rust/WASM and SQLite transactions; no public socket/auth provider, delivery fence, recovery, agent registration or MLS key-package claims.';
const sourcePaths = [
  'Cargo.toml', 'Cargo.lock', 'crates/roda-relay-admission/src/directory.rs',
  'crates/roda-relay-admission/tests/directory.rs', 'crates/roda-relay-admission/src/lib.rs',
  'crates/zoen-relay/src/db.rs', 'crates/zoen-relay/src/session.rs',
  'crates/zoen-relay/examples/cloudflare-directory-fixture.rs',
  'infra/cloudflare/relay-state/src/directory-state.mjs', 'infra/cloudflare/relay-state/src/directory-index.mjs',
  'infra/cloudflare/relay-state/tests/directory-fixture.mjs', 'infra/cloudflare/relay-state/tests/directory-state.test.mjs',
  'infra/cloudflare/relay-state/wrangler.directory.jsonc', 'infra/cloudflare/relay-state/package.json',
  'infra/cloudflare/relay-state/DIRECTORY.md', 'scripts/verify-cloudflare-directory.mjs',
];
function run(command, commandArgs, { capture = false, env = environment } = {}) {
  console.log(`Running ${command} ${commandArgs.join(' ')}`);
  return new Promise((accept, reject) => {
    const child = spawn(command, commandArgs, { cwd: root, env, stdio: ['ignore', capture ? 'pipe' : 'inherit', 'inherit'] });
    const chunks = [];
    if (capture) child.stdout.on('data', chunk => chunks.push(chunk));
    child.once('error', reject);
    child.once('close', (code, signal) => code === 0
      ? accept(capture ? Buffer.concat(chunks).toString('utf8') : undefined)
      : reject(new Error(`${command} failed (${signal ?? code})`)));
  });
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
  assert.equal(fixture.steps.length, 33, 'Every native directory scenario must run');
  const fixturePath = join(output, 'directory-native-fixtures.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
  await run(process.execPath, ['--test', 'infra/cloudflare/relay-state/tests/directory-state.test.mjs'], {
    env: { ...environment, ZOEN_DIRECTORY_FIXTURE: fixturePath },
  });
  const copied = join(output, 'wasm');
  await mkdir(copied, { recursive: true });
  for (const name of ['admission.js', 'admission_bg.wasm']) await copyFile(join(root, 'infra/cloudflare/relay-state/generated/wasm', name), join(copied, name));
  const checkoutHead = (await run('git', ['rev-parse', 'HEAD'], { capture: true })).trim();
  const dirty = Boolean((await run('git', ['status', '--porcelain', '--', ...sourcePaths], { capture: true })).trim());
  await summary({ passed: true, scope, output, skipBuild, nativeSteps: fixture.steps.length, workerdCases: 9,
    source: { commit: process.env.GITHUB_SHA ?? checkoutHead, checkoutHead, dirty, dirtyScope: sourcePaths },
    sources: await Promise.all(sourcePaths.map(path => artifact(join(root, path)))),
    artifacts: await Promise.all([fixturePath, join(copied, 'admission.js'), join(copied, 'admission_bg.wasm')].map(artifact)),
  });
  console.log(`Private directory verification passed; receipt ${join(output, 'summary.json')}`);
} catch (failure) {
  await summary({ passed: false, scope, output, skipBuild, error: failure.message });
  throw failure;
}
