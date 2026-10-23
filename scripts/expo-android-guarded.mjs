#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { constants, createReadStream } from 'node:fs';
import {
  chmod, copyFile, lstat, mkdir, mkdtemp, readFile, readdir, realpath, rename, rm, writeFile,
} from 'node:fs/promises';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ExpoCargoArtifacts } from './expo-cargo-artifacts.mjs';
import { androidCargoTooling, androidTargets, ndkArguments } from './expo-android-cargo-proxy.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const moduleRoot = join(root, 'apps/zoen/modules/zoen-core');
const digest = (value) => createHash('sha256').update(value).digest('hex');
async function fileDigest(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

export function androidOptions(args) {
  const options = { profile: 'dev', abis: ['arm64-v8a', 'x86_64'], output: join(moduleRoot, 'generated/android') };
  for (const arg of args) {
    if (arg === '--release') options.profile = 'release';
    else if (arg === '--debug') options.profile = 'dev';
    else if (arg.startsWith('--abis=')) options.abis = arg.slice(7).split(',');
    else if (arg.startsWith('--output=')) options.output = resolve(root, arg.slice(9));
    else throw new Error('Unknown Android build option');
  }
  if (!options.abis.length || new Set(options.abis).size !== options.abis.length ||
      options.abis.some((abi) => !androidTargets[abi]))
    throw new Error('Use arm64-v8a and/or x86_64 once each');
  return options;
}

async function executable(name) {
  const paths = isAbsolute(name) ? [name] : (process.env.PATH ?? '').split(':').map((p) => join(p, name));
  for (const path of paths) {
    try {
      const info = await lstat(path);
      if ((info.isFile() || info.isSymbolicLink()) && ((await lstat(await realpath(path))).mode & 0o111))
        return resolve(path); // Preserve rustup's cargo argv[0], not its resolved rustup filename.
    } catch { /* Try the next public PATH entry. */ }
  }
  throw new Error(`Missing build executable: ${name}`);
}

async function privateProxy(directory) {
  const path = join(directory, 'proxy');
  await mkdir(path, { mode: 0o700 }).catch((error) => { if (error.code !== 'EEXIST') throw error; });
  const info = await lstat(path);
  if (!info.isDirectory() || info.isSymbolicLink() || info.uid !== process.getuid() || info.mode & 0o077)
    throw new Error('Android Cargo proxy directory is not private and owned');
  const launcher = join(path, 'cargo');
  const module = new URL('./expo-android-cargo-proxy.mjs', import.meta.url).href;
  const body = `#!/usr/bin/env node\nimport(${JSON.stringify(module)}).then(({proxyMain}) => proxyMain()).catch(error => { console.error(error.message); process.exitCode = 1; });\n`;
  try {
    await writeFile(launcher, body, { flag: 'wx', mode: 0o500 });
  } catch (error) {
    if (error.code !== 'EEXIST') throw error;
    const existing = await lstat(launcher);
    if (!existing.isFile() || existing.isSymbolicLink() || existing.uid !== process.getuid() ||
        existing.nlink !== 1 || (existing.mode & 0o777) !== 0o500 || await readFile(launcher, 'utf8') !== body)
      throw new Error('Existing Android Cargo proxy does not match its source');
  }
  return launcher;
}

async function validatePreviousOutput(path) {
  try {
    const info = await lstat(path);
    if (!info.isDirectory() || info.isSymbolicLink()) throw new Error('Android output is not a regular directory');
  } catch (error) {
    if (error.code === 'ENOENT') return;
    throw error;
  }
  async function visit(directory, prefix = '') {
    for (const item of await readdir(directory, { withFileTypes: true })) {
      const name = prefix + item.name;
      if (item.isDirectory()) await visit(join(directory, item.name), name + '/');
      else if (!item.isFile() || !(
        ['core-inputs.json', 'core-contract.json', 'kotlin/xyz/tironi/zoen/core/client_core.kt'].includes(name) ||
        /^jniLibs\/(arm64-v8a|x86_64)\/libclient_core\.so$/.test(name)
      )) throw new Error('Android output contains an unowned or unexpected file');
    }
  }
  await visit(path);
}

export async function buildAndroid(args = process.argv.slice(2)) {
  const options = androidOptions(args);
  if (!process.env.CARGO_TARGET_DIR) throw new Error('Guarded Android builds require an explicit nonempty CARGO_TARGET_DIR');
  if (process.env.CARGO_NDK_OUTPUT_PATH !== undefined)
    throw new Error('Shared Android builds refuse CARGO_NDK_OUTPUT_PATH; use --output');
  if (Object.keys(process.env).some((name) => name.startsWith('_CARGO_NDK_')))
    throw new Error('Do not start an Android build inside an existing cargo-ndk linker environment');
  const target = resolve(root, process.env.CARGO_TARGET_DIR);
  const cargo = await executable('cargo');
  const ndk = await executable('cargo-ndk');
  const python = process.env.ZOEN_CARGO_GUARD_PYTHON ?? 'python3';
  await mkdir(join(moduleRoot, 'generated/cargo-source-guard'), { recursive: true });
  const proxy = await privateProxy(join(moduleRoot, 'generated/cargo-source-guard'));
  const directory = await mkdtemp(join(moduleRoot, 'generated/cargo-source-guard/android-'));
  await chmod(directory, 0o700);
  await validatePreviousOutput(options.output);
  await mkdir(dirname(options.output), { recursive: true });
  const stage = await mkdtemp(join(dirname(options.output), '.android-stage-'));
  let active;
  let cancelled = false;
  const cancel = () => {
    cancelled = true;
    if (active) {
      active.child.stdin.end();
      if (!active.controlled) active.child.kill('SIGTERM');
    }
  };
  process.on('SIGTERM', cancel);
  process.on('SIGINT', cancel);
  const execute = (program, arguments_, extra = {}, capture = false, controlled = null) => new Promise((ok, fail) => {
    if (cancelled) return fail(new Error('Android build cancelled'));
    const child = spawn(program, arguments_, {
      cwd: root, env: { ...process.env, ...extra },
      stdio: ['pipe', capture ? 'pipe' : 'inherit', 'inherit'],
    });
    active = { child, controlled };
    let output = Buffer.alloc(0);
    if (capture) child.stdout.on('data', (chunk) => {
      output = Buffer.concat([output, chunk]);
      if (output.length > 8 * 1024 * 1024) child.kill('SIGTERM');
    });
    child.once('error', fail);
    child.once('exit', async (code, signal) => {
      child.stdin.end(); // Closing the only control writer retires any still-owned NDK proxy.
      active = undefined;
      try {
        if (controlled && (code !== 0 || cancelled)) await retireProxy(controlled);
        if (code === 0 && !cancelled && output.length <= 8 * 1024 * 1024) ok(output.toString('utf8'));
        else fail(new Error(`Android build child did not complete (${signal ?? code})`));
      } catch (error) { fail(error); }
    });
  });
  const run = (program, arguments_, cwd, environment) => {
    if (cwd !== root) throw new Error('Unexpected Android build working directory');
    return execute(program, arguments_, environment);
  };
  try {
    const version = (await execute(ndk, ['ndk', '--version'], { CARGO: cargo }, true)).trim();
    if (version !== 'cargo-ndk 4.1.2')
      throw new Error('Shared Android qualification requires the reviewed cargo-ndk 4.1.2 argument contract');
    const metadata = JSON.parse(await execute(cargo, [
      'metadata', '--locked', '--offline', '--no-deps', '--format-version', '1',
    ], {}, true));
    if (await realpath(metadata.workspace_root) !== await realpath(root))
      throw new Error('Android metadata resolved another workspace');
    const generator = metadata.packages.filter((pkg) => pkg.name === 'uniffi-bindgen');
    if (generator.length !== 1) throw new Error('Expected one actual binding-generator manifest');
    await execute(python, ['scripts/android-core-contract.py', 'capture', '--output', stage,
      '--abis', options.abis.join(','), '--profile', options.profile]);
    const source = JSON.parse(await readFile(join(stage, 'core-inputs.json'), 'utf8'));
    const library = source.library;
    const manifest = resolve(root, library.manifest);
    const actualLibrary = metadata.packages.filter((pkg) => pkg.manifest_path === manifest);
    if (actualLibrary.length !== 1 || actualLibrary[0].name !== library.package)
      throw new Error('Android contract and actual Cargo metadata disagree');
    const tooling = androidCargoTooling.map((name) => join(root, 'scripts', name));
    const guarded = new ExpoCargoArtifacts({
      root, target, directory, run, python, cargo,
      inputFiles: [...tooling, cargo, ndk, join(moduleRoot, 'android/uniffi.toml')],
      inputEnvironment: ['ANDROID_HOME', 'ANDROID_SDK_ROOT', 'ANDROID_NDK_HOME', 'ANDROID_NDK_ROOT'],
    });
    const { generator: bindgen } = await guarded.build(['-p', 'uniffi-bindgen'], [{
      key: 'generator', manifest: generator[0].manifest_path, name: 'uniffi-bindgen', executable: true,
    }]);
    const hostName = process.platform === 'darwin' ? `lib${library.name}.dylib`
      : process.platform === 'linux' ? `lib${library.name}.so` : null;
    if (!hostName) throw new Error('Build the Android core on macOS or Linux');
    const { host } = await guarded.build(['-p', library.package, '--lib'], [{
      key: 'host', manifest, name: hostName,
    }]);
    await mkdir(join(stage, 'kotlin'), { recursive: true });
    await execute(bindgen, ['generate', '--library', host, '--language', 'kotlin', '--config',
      join(moduleRoot, 'android/uniffi.toml'), '--out-dir', join(stage, 'kotlin'), '--no-format']);
    await execute(python, ['scripts/fix-android-uniffi-callbacks.py', join(stage, 'kotlin')]);
    await execute('rustup', ['target', 'add', ...options.abis.map((abi) => androidTargets[abi])]);
    const abiProofs = [];
    for (const abi of options.abis) {
      const abiDirectory = join(directory, abi);
      await mkdir(abiDirectory, { mode: 0o700 });
      const context = {
        schema: 1, root: await realpath(root), target, directory: abiDirectory,
        cargo, ndk, proxy, manifest, package: library.package, library: library.name,
        abi, triple: androidTargets[abi], profile: options.profile, api: 28,
        report: join(abiDirectory, 'result.json'), sourceSha256: source.input_sha256,
      };
      const contextPath = join(abiDirectory, 'context.json');
      await writeFile(contextPath, JSON.stringify(context) + '\n', { flag: 'wx', mode: 0o400 });
      const environment = {
        CARGO: proxy,
        ZOEN_ANDROID_CARGO_CONTEXT: contextPath, ZOEN_ANDROID_CARGO_CONTROL: '1',
        CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS: '-C link-arg=-Wl,-z,max-page-size=16384',
        CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS: '-C link-arg=-Wl,-z,max-page-size=16384',
      };
      // No -o, and CARGO_NDK_OUTPUT_PATH was refused above: NDK never re-reads
      // a shared .so to publish it. The proxy owns the only admitted Cargo build.
      await execute(ndk, ['ndk', '-t', abi, '--platform', '28', ...ndkArguments(context)],
        environment, false, context.report);
      const report = JSON.parse(await readFile(context.report, 'utf8'));
      if (report.schema !== 1 || report.abi !== abi || report.triple !== context.triple ||
          report.profile !== options.profile || report.manifest !== manifest ||
          !report.library.startsWith(abiDirectory + '/') ||
          !report.qualification.startsWith(abiDirectory + '/'))
        throw new Error('NDK proxy did not return the requested private qualification');
      const qualificationRaw = await readFile(report.qualification);
      const qualification = JSON.parse(qualificationRaw);
      if (qualification.sourceSha256 !== source.input_sha256 || qualification.builds.length !== 1)
        throw new Error('NDK proxy qualification does not match this source');
      const artifact = qualification.builds[0].artifacts;
      const receipt = JSON.parse(await readFile(qualification.builds[0].receipt, 'utf8'));
      if (artifact.length !== 1 || artifact[0].path !== report.library ||
          (await fileDigest(report.library)) !== artifact[0].sha256 ||
          receipt.status !== 'PASS' || receipt.target !== await realpath(target) ||
          receipt.scope.profile !== options.profile || !sameTarget(receipt.scope.targets, context.triple))
        throw new Error('NDK copied artifact or receipt failed final validation');
      const destination = join(stage, 'jniLibs', abi);
      await mkdir(destination, { recursive: true });
      const copied = join(destination, 'libclient_core.so');
      await copyFile(report.library, copied, constants.COPYFILE_EXCL);
      if ((await fileDigest(copied)) !== artifact[0].sha256)
        throw new Error('Private Android library changed while staging');
      await chmod(copied, 0o644);
      abiProofs.push({ abi, target: context.triple, qualification: report.qualification,
        sha256: digest(qualificationRaw), ndk: report.ndk });
    }
    const hostQualification = await guarded.finish(source.input_sha256);
    const qualificationPath = join(directory, 'android-qualification.json');
    await writeFile(qualificationPath, JSON.stringify({
      schema: 'zoen.android.cargo-qualification.v1', sourceSha256: source.input_sha256,
      target, cargoNdkVersion: version,
      host: { path: hostQualification, sha256: digest(await readFile(hostQualification)) },
      abis: abiProofs,
      sdkBoundary: 'Pinned NDK installation assumed immutable; manual unversioned sysroot edits are not attested.',
    }, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
    await execute(python, ['scripts/android-core-contract.py', 'finish', '--output', stage,
      '--host-library', host, '--abis', options.abis.join(','), '--profile', options.profile,
      '--cargo-qualification', qualificationPath]);
    if (cancelled) throw new Error('Android build cancelled');
    await validatePreviousOutput(options.output);
    const previous = stage + '.previous';
    let movedPrevious = false;
    try {
      await rename(options.output, previous);
      movedPrevious = true;
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    try {
      await rename(stage, options.output);
    } catch (error) {
      if (movedPrevious) await rename(previous, options.output);
      throw error;
    }
    if (movedPrevious) await rm(previous, { recursive: true });
    console.log(`Android core ready (${options.abis.join(',')}, ${options.profile}); qualification: ${qualificationPath}`);
  } finally {
    if (active) {
      active.child.stdin.end();
      if (!active.controlled) active.child.kill('SIGTERM');
    }
    process.removeListener('SIGTERM', cancel);
    process.removeListener('SIGINT', cancel);
    await rm(stage, { recursive: true, force: true });
  }
}

// Only a cleanup observation: expiry refuses output and never retries or claims
// that an unresponsive/SIGKILLed proxy was reaped. The existing guard owns Cargo.
async function retireProxy(report) {
  let started;
  try { started = JSON.parse(await readFile(report + '.started', 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return; throw error; }
  if (!Number.isSafeInteger(started.pid) || started.pid <= 1)
    throw new Error('Invalid owned Android proxy cleanup identity');
  const deadline = performance.now() + 10_000;
  while (performance.now() < deadline) {
    try {
      const retired = JSON.parse(await readFile(report + '.retired', 'utf8'));
      if (retired.pid !== started.pid) throw new Error('Android proxy cleanup identity changed');
      try { process.kill(started.pid, 0); }
      catch (error) { if (error.code === 'ESRCH') return; throw error; }
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    await new Promise((done) => setTimeout(done, 20));
  }
  throw new Error('Owned Android proxy cleanup was not confirmed; no output promotion');
}

function sameTarget(targets, expected) {
  return Array.isArray(targets) && targets.length === 1 && targets[0] === expected;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  buildAndroid().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
