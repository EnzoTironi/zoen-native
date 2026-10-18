#!/usr/bin/env node
import { execFile, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cp, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const moduleRoot = join(root, 'apps/zoen/modules/zoen-core');
const target = process.env.CARGO_TARGET_DIR ?? join(root, 'target');
const platform = process.argv[2] ?? 'types';
const profile = process.argv.includes('--release') ? 'release' : 'dev';
const iosArchitectures = process.argv.includes('--simulator')
  ? ['aarch64-apple-ios-sim']
  : ['aarch64-apple-ios', 'aarch64-apple-ios-sim'];
const androidAbis = process.env.ZOEN_ANDROID_ABIS ?? 'arm64-v8a,x86_64';
const folder = profile === 'dev' ? 'debug' : 'release';
const env = {
  ...process.env,
  PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}`,
  DEVELOPER_DIR: process.env.DEVELOPER_DIR ?? '/Applications/Xcode.app/Contents/Developer',
  CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? '2',
  CARGO_PROFILE_DEV_DEBUG: '0',
  CARGO_INCREMENTAL: '0',
};

function run(command, args, cwd = root, extra = {}) {
  return new Promise((ok, fail) => {
    const child = spawn(command, args, { cwd, stdio: 'inherit', env: { ...env, ...extra } });
    child.once('error', fail);
    child.once('exit', (code) =>
      code === 0 ? ok() : fail(new Error(`${command} exited ${code}`)),
    );
  });
}

async function inputs(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const result = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) result.push(...(await inputs(path)));
    else if (/\.(rs|toml|json|html)$/.test(path)) result.push(path);
  }
  return result;
}
async function sourceHash() {
  const hash = createHash('sha256');
  hash.update(JSON.stringify({ platform, profile, iosArchitectures, androidAbis }));
  // The browser core evolves independently. Hash the actual native dependency
  // closure, including the binding generator, rather than every workspace crate.
  const { stdout } = await promisify(execFile)(
    'cargo',
    ['metadata', '--locked', '--offline', '--no-deps', '--format-version', '1'],
    { cwd: root, env, maxBuffer: 8 * 1024 * 1024 },
  );
  const packages = JSON.parse(stdout).packages;
  const byDirectory = new Map(packages.map((pkg) => [dirname(pkg.manifest_path), pkg]));
  const pending = ['roda-ffi', ...(platform === 'types' ? [] : ['uniffi-bindgen'])].map((name) => {
    const pkg = packages.find((candidate) => candidate.name === name);
    if (!pkg) throw new Error(`Missing build package: ${name}`);
    return dirname(pkg.manifest_path);
  });
  const directories = new Set();
  while (pending.length) {
    const directory = pending.pop();
    if (directories.has(directory)) continue;
    const pkg = byDirectory.get(directory);
    if (!pkg) throw new Error(`Missing local dependency metadata: ${directory}`);
    directories.add(directory);
    for (const dependency of pkg.dependencies) {
      if (dependency.path && (platform === 'types' || dependency.kind !== 'dev'))
        pending.push(resolve(dependency.path));
    }
  }
  const files = [
    ...(await Promise.all([...directories].map(inputs))).flat(),
    join(root, 'Cargo.toml'),
    join(root, 'Cargo.lock'),
    fileURLToPath(import.meta.url),
    ...(platform === 'android'
      ? ['build-android-core.sh', 'android-core-contract.py', 'fix-android-uniffi-callbacks.py'].map((name) => join(root, 'scripts', name))
      : []),
  ];
  for (const path of files.sort()) {
    hash.update(path.slice(root.length));
    hash.update(await readFile(path));
  }
  return hash.digest('hex');
}

if (!['ios', 'android', 'types'].includes(platform))
  throw new Error('usage: build-expo-core.mjs ios|android|types [--release]');
const hash = await sourceHash();
// One stamp describes the artifact currently installed at the shared output path.
const stamp = join(moduleRoot, 'generated', `${platform}.json`);
try {
  const previous = JSON.parse(await readFile(stamp, 'utf8'));
  // TypeScript is checked in and can drift independently from the Rust inputs.
  // Always rerun its inexpensive exporter; Cargo already caches the executable.
  if (platform !== 'types' && previous.sha256 === hash) {
    await Promise.all(previous.outputs.map((path) => stat(path)));
    process.exit(0);
  }
} catch {
  /* First build or a changed input. */
}
await mkdir(join(moduleRoot, 'generated'), { recursive: true });

let outputs;
if (platform === 'types') {
  const out = join(root, 'apps/zoen/src/core/generated');
  await run('cargo', [
    'run',
    '--locked',
    '-p',
    'roda-ffi',
    '--example',
    'export-client-types',
    '--features',
    'client-types',
    '--',
    out,
  ]);
  outputs = [join(out, 'Command.ts'), join(out, 'Snapshot.ts')];
} else if (platform === 'android') {
  const out = join(moduleRoot, 'generated/android');
  const abis = androidAbis;
  await run('bash', [
    'scripts/build-android-core.sh',
    profile === 'release' ? '--release' : '--debug',
    `--abis=${abis}`,
    `--output=${out}`,
  ]);
  outputs = [
    join(out, 'kotlin'),
    ...abis.split(',').map((abi) => join(out, 'jniLibs', abi, 'libroda_ffi.so')),
  ];
} else {
  const out = join(moduleRoot, 'ios/generated');
  const binding = join(moduleRoot, 'generated/swift');
  await mkdir(binding, { recursive: true });
  await mkdir(join(binding, 'headers'), { recursive: true });
  await mkdir(out, { recursive: true });
  await run('cargo', ['build', '--locked', '-p', 'roda-ffi', '--profile', profile]);
  for (const architecture of iosArchitectures) {
    await run(
      'cargo',
      ['build', '--locked', '-p', 'roda-ffi', '--profile', profile, '--target', architecture],
      root,
      { IPHONEOS_DEPLOYMENT_TARGET: '26.0' },
    );
  }
  await run('cargo', [
    'run',
    '--locked',
    '-p',
    'uniffi-bindgen',
    '--',
    'generate',
    '--library',
    join(target, folder, 'libroda_ffi.dylib'),
    '--language',
    'swift',
    '--out-dir',
    binding,
  ]);
  await cp(join(binding, 'roda_ffiFFI.h'), join(binding, 'headers/roda_ffiFFI.h'));
  await cp(join(binding, 'roda_ffiFFI.modulemap'), join(binding, 'headers/module.modulemap'));
  const framework = join(out, 'RodaFFI.xcframework');
  await rm(framework, { recursive: true, force: true });
  await run('xcodebuild', [
    '-create-xcframework',
    ...iosArchitectures.flatMap((architecture) => [
      '-library',
      join(target, architecture, folder, 'libroda_ffi.a'),
      '-headers',
      join(binding, 'headers'),
    ]),
    '-output',
    framework,
  ]);
  await cp(join(binding, 'roda_ffi.swift'), join(out, 'RodaFFI.generated.swift'));
  outputs = [join(framework, 'Info.plist'), join(out, 'RodaFFI.generated.swift')];
}
await writeFile(
  stamp,
  `${JSON.stringify({ sha256: hash, profile, platform, outputs }, null, 2)}\n`,
);
