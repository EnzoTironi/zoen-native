import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cp, mkdir, mkdtemp, readFile, readdir, realpath, rm, stat, utimes, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';
import test from 'node:test';
import { promisify } from 'node:util';
import { androidCargoTooling, androidTargets, ndkArguments } from '../expo-android-cargo-proxy.mjs';
import { cargoTooling } from '../expo-cargo-artifacts.mjs';

const execute = promisify(execFile);
const source = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const cargo = process.env.CARGO_TEST_BIN ?? join(process.env.HOME, '.cargo/bin/cargo');
const ndk = process.env.CARGO_NDK_TEST_BIN ?? join(process.env.HOME, '.cargo/bin/cargo-ndk');
const python = process.env.ZOEN_CARGO_GUARD_PYTHON ?? 'python3';
const enabled = process.env.ZOEN_TEST_ANDROID_SHARED_CARGO === '1';
const hash = (value) => createHash('sha256').update(value).digest('hex');
const options = { skip: !enabled && 'Requires explicit Android NDK/Rust fixture admission', timeout: 120_000 };

async function write(path, value, mode) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, value, mode ? { mode } : undefined);
}
async function age(path) {
  for (const entry of await readdir(path, { withFileTypes: true })) {
    if (entry.name === '.git') continue;
    const name = join(path, entry.name);
    if (entry.isDirectory()) await age(name);
    else await utimes(name, 1_000_000_000, 1_000_000_000);
  }
}
async function fixture(t) {
  const base = process.env.EXPO_CARGO_TEST_ROOT ?? tmpdir();
  await mkdir(base, { recursive: true });
  const root = await realpath(await mkdtemp(join(base, 'ndk-guard-')));
  if (!process.env.EXPO_CARGO_TEST_ROOT) t.after(() => rm(root, { recursive: true, force: true }));
  return root;
}
async function workspace(root, marker) {
  for (const name of new Set([...cargoTooling, ...androidCargoTooling])) {
    await mkdir(join(root, 'scripts'), { recursive: true });
    await cp(join(source, 'scripts', name), join(root, 'scripts', name));
  }
  await write(join(root, 'Cargo.toml'), '[workspace]\nmembers=["engines/probe"]\nresolver="2"\n');
  await write(join(root, '.gitignore'), '/private/\n');
  await write(join(root, 'engines/probe/Cargo.toml'),
    '[package]\nname="android-probe"\nversion="0.1.0"\nedition="2021"\n[lib]\ncrate-type=["cdylib"]\n');
  await write(join(root, 'engines/probe/build.rs'), `fn main() {
    if let Ok(path) = std::env::var("CARGO_NDK_TEST_HOLD") {
      std::fs::write(format!("{path}.started"), std::process::id().to_string()).unwrap();
      while !std::path::Path::new(&path).exists() {
        std::thread::sleep(std::time::Duration::from_millis(10));
      }
    }
    let selection = format!("{}|{}|{}", std::env::var("ANDROID_ABI").unwrap(),
      std::env::var("ANDROID_PLATFORM").unwrap(), std::env::var("CFLAGS").unwrap_or_default());
    println!("cargo:rustc-env=PROBE_SELECTION={selection}");
    // Deliberately no rerun-if-env-changed: the guard must invalidate env drift.
    println!("cargo:rerun-if-changed=build.rs");
  }\n`);
  await library(root, marker);
  await execute(cargo, ['generate-lockfile', '--offline'], { cwd: root });
  await execute('git', ['init', '-q', root]);
  await execute('git', ['add', '.'], { cwd: root });
  await age(root);
  const metadata = JSON.parse((await execute(cargo, ['metadata', '--locked', '--offline',
    '--no-deps', '--format-version', '1'], { cwd: root })).stdout);
  const pkg = metadata.packages.find((value) => value.name === 'android-probe');
  assert(pkg, 'Derive the actual fixture manifest through real Cargo metadata');
  return pkg.manifest_path;
}
async function library(root, marker) {
  await write(join(root, 'engines/probe/src/lib.rs'), `
    #[no_mangle]
    pub extern "C" fn source_witness() -> *const u8 {
      concat!("${marker}|", env!("CARGO_MANIFEST_DIR"), "|", env!("PROBE_SELECTION")).as_ptr()
    }\n`);
}
async function invocation(root, target, manifest, extra = {}, abi = 'arm64-v8a', platform = '28') {
  await mkdir(join(root, 'private'), { recursive: true });
  const directory = await mkdtemp(join(root, 'private/attempt-'));
  // A stable launcher path prevents the test itself from manufacturing env drift.
  const proxy = join(root, 'private/cargo');
  await mkdir(dirname(proxy), { recursive: true });
  try {
    await writeFile(proxy, `#!${process.execPath}\nimport(${JSON.stringify(pathToFileURL(join(root, 'scripts/expo-android-cargo-proxy.mjs')).href)}).then(({proxyMain})=>proxyMain()).catch(e=>{console.error(e.message);process.exitCode=1;});\n`, { flag: 'wx', mode: 0o500 });
  } catch (error) { if (error.code !== 'EEXIST') throw error; }
  const context = { schema: 1, root, target, directory, cargo, ndk, proxy, manifest,
    package: 'android-probe', library: 'android_probe', abi, triple: androidTargets[abi],
    api: 28, profile: 'dev', report: join(directory, 'result.json'), sourceSha256: hash(await readFile(manifest)),
  };
  const contextPath = join(directory, 'context.json');
  await write(contextPath, JSON.stringify(context), 0o400);
  const env = { ...process.env, CARGO: proxy, CARGO_TARGET_DIR: target, CARGO_BUILD_BUILD_DIR: target,
    CARGO_BUILD_JOBS: '1', CARGO_INCREMENTAL: '0', CARGO_PROFILE_DEV_DEBUG: '0',
    ZOEN_CARGO_GUARD_PYTHON: python, ZOEN_ANDROID_CARGO_CONTEXT: contextPath,
    ZOEN_ANDROID_CARGO_CONTROL: '1', ...extra,
  };
  delete env.CARGO_NDK_OUTPUT_PATH;
  const child = spawn(ndk, ['ndk', '-t', abi, '--platform', platform, ...ndkArguments(context)],
    { cwd: root, env, stdio: ['pipe', 'pipe', 'pipe'] });
  const output = [];
  child.stdout.on('data', (value) => output.push(value));
  child.stderr.on('data', (value) => output.push(value));
  const terminal = new Promise((ok, fail) => {
    child.once('error', fail);
    child.once('exit', (code, signal) => {
      child.stdin.end();
      ok({ code, signal, output: Buffer.concat(output).toString() });
    });
  });
  return { child, terminal, context };
}
async function finish(invocation) {
  const result = await invocation.terminal;
  assert.equal(result.code, 0, result.output);
  const report = JSON.parse(await readFile(invocation.context.report, 'utf8'));
  const qualification = JSON.parse(await readFile(report.qualification, 'utf8'));
  const build = qualification.builds[0];
  const receipt = JSON.parse(await readFile(build.receipt, 'utf8'));
  assert.equal(receipt.status, 'PASS');
  assert.equal(receipt.scope.profile, 'dev');
  assert.deepEqual(receipt.scope.targets, [invocation.context.triple]);
  assert.equal(build.artifacts.length, 1);
  assert.equal(build.artifacts[0].path, report.library);
  const bytes = await readFile(report.library);
  assert.equal(hash(bytes), build.artifacts[0].sha256);
  assert.equal((await stat(report.library)).mode & 0o777, 0o400);
  return { receipt, bytes, report, qualification };
}
async function ownedSidecar(target) {
  const actual = await realpath(target);
  for (const name of await readdir(dirname(target))) {
    if (!name.startsWith('.cargo-source-guard-')) continue;
    const directory = join(dirname(target), name);
    const state = JSON.parse(await readFile(join(directory, 'state.json'), 'utf8'));
    if (state.target === actual) return directory;
  }
  throw new Error('No canonical source stamp for this fixture target');
}
async function waitFor(path, predicate = () => true) {
  const deadline = performance.now() + 20_000;
  while (performance.now() < deadline) {
    try { const value = await readFile(path, 'utf8'); if (predicate(value)) return value; }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    await new Promise((ok) => setTimeout(ok, 20));
  }
  throw new Error('Owned tiny fixture did not publish its readiness before the harness bound');
}
function alive(pid) {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === 'ESRCH') return false; throw error; }
}

// These compile only a dependency-free synthetic cdylib through the actual
// cargo-ndk and canonical guard. They do not build Core or run Android code.
test('actual NDK A → B old-mtime, restored-mtime edit, env and ABI identities use private qualified ELF copies', options, async (t) => {
  const path = await fixture(t), a = join(path, 'a'), b = join(path, 'b'), target = join(path, 'shared');
  const am = await workspace(a, 'A'), bm = await workspace(b, 'B');
  const qa = await finish(await invocation(a, target, am));
  assert(qa.bytes.includes(Buffer.from(`A|${dirname(am)}|arm64-v8a|28|`)));
  const qb = await finish(await invocation(b, target, bm));
  assert(qb.bytes.includes(Buffer.from(`B|${dirname(bm)}|arm64-v8a|28|`)));
  assert(qb.receipt.cleanedPackages.length > 0);
  assert(qb.receipt.artifacts.every((artifact) => artifact.manifest === bm));
  const copyA = await readFile(qa.report.library);
  assert.deepEqual(copyA, qa.bytes, 'A copy survives replacement of shared outputs by B');
  const fresh = await finish(await invocation(b, target, bm));
  assert.deepEqual(fresh.receipt.cleanedPackages, []);
  assert(fresh.receipt.artifacts.every((artifact) => artifact.fresh));
  const original = await stat(join(b, 'engines/probe/src/lib.rs'));
  await library(b, 'C');
  await utimes(join(b, 'engines/probe/src/lib.rs'), original.atime, original.mtime);
  const qc = await finish(await invocation(b, target, bm));
  assert(qc.bytes.includes(Buffer.from(`C|${dirname(bm)}|arm64-v8a|28|`)));
  assert(qc.receipt.cleanedPackages.length > 0);
  const flags = await finish(await invocation(b, target, bm, { CFLAGS: '-DPROBE_SELECTION=1' }));
  assert(flags.receipt.cleanedPackages.length > 0);
  assert(flags.bytes.includes(Buffer.from('arm64-v8a|28|-DPROBE_SELECTION=1')));
  if (process.env.ZOEN_TEST_ANDROID_ABIS === 'both') {
    const x86 = await finish(await invocation(b, target, bm, {}, 'x86_64'));
    assert.equal(x86.bytes.readUInt16LE(18), 62);
    assert(x86.bytes.includes(Buffer.from('x86_64|28|')));
  }
  assert.equal(qc.bytes.readUInt16LE(18), 183);
});

test('actual NDK refuses an unknown nonempty cache and a held canonical lease without adoption or private artifacts', options, async (t) => {
  const path = await fixture(t), root = join(path, 'work'), target = join(path, 'unknown');
  const manifest = await workspace(root, 'refuse');
  await write(join(target, 'keep'), 'unmanaged');
  const unknown = await invocation(root, target, manifest);
  assert.notEqual((await unknown.terminal).code, 0);
  await assert.rejects(readFile(unknown.context.report), { code: 'ENOENT' });
  assert.equal(await readFile(join(target, 'keep'), 'utf8'), 'unmanaged');
  const wrongApi = await invocation(root, join(path, 'api-refused'), manifest, {}, 'arm64-v8a', '29');
  const apiResult = await wrongApi.terminal;
  assert.notEqual(apiResult.code, 0);
  assert.match(apiResult.output, /ABI\/API environment differs/);
  await assert.rejects(readFile(wrongApi.context.report), { code: 'ENOENT' });
  const shared = join(path, 'shared');
  await finish(await invocation(root, shared, manifest));
  const sidecar = await ownedSidecar(shared);
  const ready = join(path, 'held');
  const holder = spawn(python, ['-c', 'import fcntl, pathlib, sys; f=open(sys.argv[1],"r+"); fcntl.flock(f,fcntl.LOCK_EX); pathlib.Path(sys.argv[2]).touch(); sys.stdin.read()', join(sidecar, 'lease'), ready], { stdio: ['pipe', 'ignore', 'inherit'] });
  const terminal = new Promise((ok, fail) => { holder.once('exit', ok); holder.once('error', fail); });
  try {
    await waitFor(ready);
    const busy = await invocation(root, shared, manifest);
    const result = await busy.terminal;
    assert.notEqual(result.code, 0);
    assert.match(result.output, /Target is busy/);
    await assert.rejects(readFile(busy.context.report), { code: 'ENOENT' });
  } finally { holder.stdin.end(); await terminal; }
});

test('owned NDK cancellation closes its control pipe, retires proxy/guard/Cargo and never qualifies a partial library', options, async (t) => {
  const path = await fixture(t), root = join(path, 'work'), target = join(path, 'shared');
  const manifest = await workspace(root, 'cancel');
  const hold = join(path, 'release');
  const running = await invocation(root, target, manifest, { CARGO_NDK_TEST_HOLD: hold });
  let buildPid, proxyPid;
  try {
    buildPid = Number(await waitFor(hold + '.started'));
    proxyPid = JSON.parse(await waitFor(running.context.report + '.started')).pid;
    assert(alive(buildPid));
    assert(alive(proxyPid));
    running.child.kill('SIGTERM');
    const result = await running.terminal; // closes the only control writer
    assert.notEqual(result.code, 0);
    await waitFor(running.context.report + '.retired');
    const deadline = performance.now() + 10_000;
    while ((alive(proxyPid) || alive(buildPid)) && performance.now() < deadline)
      await new Promise((ok) => setTimeout(ok, 20));
    assert(!alive(proxyPid));
    assert(!alive(buildPid));
    await assert.rejects(readFile(running.context.report), { code: 'ENOENT' });
    const sidecar = await ownedSidecar(target);
    assert.notEqual(JSON.parse(await readFile(join(sidecar, 'state.json'))).status, 'bound');
  } finally {
    running.child.stdin.end();
    running.child.kill('SIGTERM');
    await write(hold, 'release');
    await running.terminal;
    if (proxyPid) await waitFor(running.context.report + '.retired');
    // The canonical guard owns process escalation. Never kill a later process
    // merely because it has reused a previously recorded numeric PID.
  }
});
