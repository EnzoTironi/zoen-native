#!/usr/bin/env node
import { execFile, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cp, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { cargoTooling, ExpoCargoArtifacts } from './expo-cargo-artifacts.mjs';

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
const sharedTarget = process.env.CARGO_TARGET_DIR !== undefined;
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

const guarded = sharedTarget && platform !== 'android'
  ? new ExpoCargoArtifacts({
      root,
      target,
      directory: join(moduleRoot, 'generated/cargo-source-guard'),
      run,
      python: process.env.ZOEN_CARGO_GUARD_PYTHON,
    })
  : null;

async function inputs(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const result = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) result.push(...(await inputs(path)));
    else if (/\.(rs|toml|json|html|deckcraft)$/.test(path)) result.push(path);
  }
  return result;
}

async function exactFiles(directory, expected) {
  const actual = [];
  async function visit(path, prefix = '') {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      const name = prefix + entry.name;
      if (entry.isDirectory()) await visit(join(path, entry.name), `${name}/`);
      else if (entry.isFile()) actual.push(name);
      else throw new Error(`Unexpected generated artifact: ${join(path, entry.name)}`);
    }
  }
  await visit(directory);
  if (JSON.stringify(actual.sort()) !== JSON.stringify(expected.sort()))
    throw new Error(`Generated output inventory changed: ${directory}`);
}

async function validateGeneratedInventory() {
  if (platform === 'android') {
    const out = join(moduleRoot, 'generated/android');
    await exactFiles(out, [
      'core-inputs.json',
      'core-contract.json',
      'kotlin/xyz/tironi/zoen/core/client_core.kt',
      ...androidAbis.split(',').map((abi) => `jniLibs/${abi}/libclient_core.so`),
    ]);
  } else if (platform === 'ios') {
    await exactFiles(join(moduleRoot, 'ios/generated'), [
      'ClientFFI.generated.swift',
      'ClientFFI.xcframework/Info.plist',
      ...iosArchitectures.flatMap((architecture) => {
        const slice = architecture.endsWith('-sim') ? 'ios-arm64-simulator' : 'ios-arm64';
        const prefix = `ClientFFI.xcframework/${slice}/`;
        return [
          `${prefix}libclient_core.a`,
          `${prefix}Headers/client_coreFFI.h`,
          `${prefix}Headers/module.modulemap`,
        ];
      }),
    ]);
    await exactFiles(join(moduleRoot, 'generated/swift'), [
      'client_core.swift',
      'client_coreFFI.h',
      'client_coreFFI.modulemap',
      'headers/client_coreFFI.h',
      'headers/module.modulemap',
    ]);
  }
}
async function sourceInputs() {
  const hash = createHash('sha256');
  hash.update(
    JSON.stringify({
      platform,
      profile,
      iosArchitectures,
      androidAbis,
      // The registry is compiled into Rust. A config-only change must not reuse
      // native artifacts from another verified code package or deployment URL.
      deckcraftResourceManifest: process.env.ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON ?? null,
      gridcraftResourceManifest: process.env.ZOEN_GRIDCRAFT_RESOURCE_MANIFEST_JSON ?? null,
    }),
  );
  // The browser core evolves independently. Hash the actual native dependency
  // closure, including the binding generator, rather than every workspace crate.
  const { stdout } = await promisify(execFile)(
    'cargo',
    ['metadata', '--locked', '--offline', '--no-deps', '--format-version', '1'],
    { cwd: root, env, maxBuffer: 8 * 1024 * 1024 },
  );
  const packages = JSON.parse(stdout).packages;
  const byDirectory = new Map(packages.map((pkg) => [dirname(pkg.manifest_path), pkg]));
  const manifests = new Map();
  const pending = ['client-core', ...(platform === 'types' ? [] : ['uniffi-bindgen'])].map(
    (name) => {
      const pkg = packages.find((candidate) => candidate.name === name);
      if (!pkg) throw new Error(`Missing build package: ${name}`);
      manifests.set(name, pkg.manifest_path);
      return dirname(pkg.manifest_path);
    },
  );
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
    ...cargoTooling.map((name) => join(root, 'scripts', name)),
    ...(platform === 'android'
      ? [
          ...[
            'build-android-core.sh',
            'android-core-contract.py',
            'fix-android-uniffi-callbacks.py',
            'expo-android-guarded.mjs',
            'expo-android-cargo-proxy.mjs',
          ].map((name) => join(root, 'scripts', name)),
          join(moduleRoot, 'android/uniffi.toml'),
        ]
      : []),
  ];
  for (const path of files.sort()) {
    hash.update(path.slice(root.length));
    hash.update(await readFile(path));
  }
  return { sha256: hash.digest('hex'), manifests };
}

if (!['ios', 'android', 'types'].includes(platform))
  throw new Error('usage: build-expo-core.mjs ios|android|types [--release]');
const { sha256: hash, manifests } = await sourceInputs();
// One stamp describes the artifact currently installed at the shared output path.
const stamp = join(moduleRoot, 'generated', `${platform}.json`);
try {
  const previous = JSON.parse(await readFile(stamp, 'utf8'));
  // TypeScript is checked in and can drift independently from the Rust inputs.
  // Always rerun its inexpensive exporter; Cargo already caches the executable.
  if (!sharedTarget && platform !== 'types' && previous.sha256 === hash) {
    await Promise.all(previous.outputs.map((path) => stat(path)));
    await validateGeneratedInventory();
    process.exit(0);
  }
} catch {
  /* First build or a changed input. */
}
await mkdir(join(moduleRoot, 'generated'), { recursive: true });

let outputs;
if (platform === 'types') {
  const out = join(root, 'apps/zoen/src/core/generated');
  if (guarded) {
    const { exporter } = await guarded.build(
      ['-p', 'client-core', '--example', 'export-client-types', '--features', 'client-types'],
      [
        {
          key: 'exporter',
          manifest: manifests.get('client-core'),
          name: 'export-client-types',
          executable: true,
        },
      ],
    );
    await run(exporter, [out]);
  } else
    await run('cargo', [
      'run',
      '--locked',
      '-p',
      'client-core',
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
    ...abis.split(',').map((abi) => join(out, 'jniLibs', abi, 'libclient_core.so')),
  ];
} else {
  const out = join(moduleRoot, 'ios/generated');
  const binding = join(moduleRoot, 'generated/swift');
  async function appleSdk(name) {
    const { stdout } = await promisify(execFile)('xcrun', ['--sdk', name, '--show-sdk-path'], {
      cwd: root,
      env,
    });
    const path = stdout.trim();
    if (!path) throw new Error(`Missing Apple SDK: ${name}`);
    return path;
  }
  // Xcode and shell sessions can export a different SDKROOT. The host binding
  // generator and each Apple target must link against their own SDK explicitly.
  const hostSdk = await appleSdk('macosx');
  let hostLibrary = join(target, folder, 'libclient_core.dylib');
  const libraries = new Map();
  if (guarded) {
    const host = await guarded.build(
      ['-p', 'client-core', '--profile', profile],
      [{ key: 'host', manifest: manifests.get('client-core'), name: 'libclient_core.dylib' }],
      { SDKROOT: hostSdk },
    );
    hostLibrary = host.host;
  } else
    await run('cargo', ['build', '--locked', '-p', 'client-core', '--profile', profile], root, {
      SDKROOT: hostSdk,
    });
  for (const architecture of iosArchitectures) {
    const options = {
      IPHONEOS_DEPLOYMENT_TARGET: '26.0',
      SDKROOT: await appleSdk(architecture.endsWith('-sim') ? 'iphonesimulator' : 'iphoneos'),
    };
    if (guarded) {
      const { library } = await guarded.build(
        ['-p', 'client-core', '--profile', profile, '--target', architecture],
        [{ key: 'library', manifest: manifests.get('client-core'), name: 'libclient_core.a' }],
        options,
      );
      libraries.set(architecture, library);
    } else
      await run(
        'cargo',
        ['build', '--locked', '-p', 'client-core', '--profile', profile, '--target', architecture],
        root,
        options,
      );
  }
  let generator;
  if (guarded)
    ({ generator } = await guarded.build(
      ['-p', 'uniffi-bindgen'],
      [
        {
          key: 'generator',
          manifest: manifests.get('uniffi-bindgen'),
          name: 'uniffi-bindgen',
          executable: true,
        },
      ],
      { SDKROOT: hostSdk },
    ));
  // Admission/build failure must not erase a previous generated framework.
  await rm(out, { recursive: true, force: true });
  await rm(binding, { recursive: true, force: true });
  await mkdir(binding, { recursive: true });
  await mkdir(join(binding, 'headers'), { recursive: true });
  await mkdir(out, { recursive: true });
  await run(
    generator ?? 'cargo',
    [
      ...(generator ? [] : ['run', '--locked', '-p', 'uniffi-bindgen', '--']),
      'generate',
      '--library',
      hostLibrary,
      '--language',
      'swift',
      '--out-dir',
      binding,
    ],
    root,
    { SDKROOT: hostSdk },
  );
  await cp(join(binding, 'client_coreFFI.h'), join(binding, 'headers/client_coreFFI.h'));
  await cp(join(binding, 'client_coreFFI.modulemap'), join(binding, 'headers/module.modulemap'));
  const framework = join(out, 'ClientFFI.xcframework');
  await rm(framework, { recursive: true, force: true });
  await run('xcodebuild', [
    '-create-xcframework',
    ...iosArchitectures.flatMap((architecture) => [
      '-library',
      libraries.get(architecture) ?? join(target, architecture, folder, 'libclient_core.a'),
      '-headers',
      join(binding, 'headers'),
    ]),
    '-output',
    framework,
  ]);
  await cp(join(binding, 'client_core.swift'), join(out, 'ClientFFI.generated.swift'));
  outputs = [join(framework, 'Info.plist'), join(out, 'ClientFFI.generated.swift')];
}
await validateGeneratedInventory();
if (sharedTarget && (await sourceInputs()).sha256 !== hash)
  throw new Error('Build inputs changed; generated outputs remain unqualified');
const cargoQualification = guarded
  ? await guarded.finish(hash)
  : sharedTarget && platform === 'android'
    ? JSON.parse(await readFile(join(moduleRoot, 'generated/android/core-contract.json'), 'utf8'))
        .cargo_qualification?.path
    : undefined;
if (sharedTarget && !cargoQualification)
  throw new Error('Shared build produced no Cargo qualification');
await writeFile(
  stamp,
  `${JSON.stringify({ sha256: hash, profile, platform, outputs, cargoQualification }, null, 2)}\n`,
);
