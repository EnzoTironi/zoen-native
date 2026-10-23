#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { constants } from 'node:fs';
import { open, realpath, stat, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ExpoCargoArtifacts } from './expo-cargo-artifacts.mjs';

export const androidTargets = {
  'arm64-v8a': 'aarch64-linux-android',
  x86_64: 'x86_64-linux-android',
};
export const androidCargoTooling = [
  'build-android-core.sh',
  'android-core-contract.py',
  'fix-android-uniffi-callbacks.py',
  'expo-android-guarded.mjs',
  'expo-android-cargo-proxy.mjs',
];

const metadataArguments = ['metadata', '--format-version', '1', '--no-deps'];
const versionArguments = new Set(['-V', '-Vv', '--version']);
const environmentNames = [
  'CARGO', 'PATH', 'ANDROID_HOME', 'ANDROID_SDK_ROOT', 'ANDROID_NDK_HOME',
  'ANDROID_NDK_ROOT', 'NDK_HOME', 'NDK_ROOT', 'ANDROID_PLATFORM', 'ANDROID_ABI',
  'CARGO_NDK_OUTPUT_PATH', 'CARGO_NDK_PLATFORM', 'CARGO_NDK_TARGET',
  '_CARGO_NDK_LINK_TARGET', '_CARGO_NDK_LINK_CLANG', '_CARGO_NDK_LDFLAGS',
  'CARGO_NDK_CMAKE_TOOLCHAIN_PATH', 'CLANG_PATH', 'LIBCLANG_PATH',
  'CC', 'CXX', 'AR', 'RANLIB', 'CFLAGS', 'CXXFLAGS', 'LDFLAGS', 'CPATH',
];
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

export function ndkArguments(context) {
  return [
    'build', '--locked', '-q', '-p', context.package, '--lib', '--profile', context.profile,
  ];
}

// cargo-ndk 4.1.2 appends this exact suffix after its caller's arguments. Only
// these controlled repetitions are removed; all compiler/linker flags stay in
// the actual inherited NDK environment, which the canonical guard fingerprints.
export function admittedArguments(context, args) {
  const expected = [
    ...ndkArguments(context), '--manifest-path', context.manifest,
    '--target', context.triple, '--message-format', 'json-render-diagnostics',
  ];
  if (!same(args, expected)) throw new Error('Unexpected cargo-ndk build arguments');
  // The canonical guard supplies --locked itself; remove only this fixed
  // duplicate, never a caller-selected compiler or linkage option.
  return [...ndkArguments(context).slice(2), '--target', context.triple];
}

export async function ndkInputs(context, env) {
  if (env.CARGO_NDK_OUTPUT_PATH !== undefined)
    throw new Error('Shared Android builds refuse CARGO_NDK_OUTPUT_PATH; use --output');
  if (env.ANDROID_ABI !== context.abi || env.ANDROID_PLATFORM !== String(context.api) ||
      env._CARGO_NDK_LINK_TARGET !== `--target=${context.triple}${context.api}`)
    throw new Error('cargo-ndk ABI/API environment differs from the requested build');
  const linker = env[`CARGO_TARGET_${context.triple.replaceAll('-', '_').toUpperCase()}_LINKER`];
  if (!linker || await realpath(linker) !== await realpath(context.ndk))
    throw new Error('cargo-ndk linker does not match the selected executable');
  const clang = env._CARGO_NDK_LINK_CLANG;
  if (!clang || env.CLANG_PATH !== clang || !env.CARGO_NDK_SYSROOT_PATH)
    throw new Error('Missing cargo-ndk compiler/sysroot environment');
  const cmake = env.CARGO_NDK_CMAKE_TOOLCHAIN_PATH;
  if (!cmake || !isAbsolute(cmake) || !cmake.endsWith('/build/cmake/android.toolchain.cmake'))
    throw new Error('Missing cargo-ndk installation selection');
  const ndk = await realpath(dirname(dirname(dirname(cmake))));
  const sysroot = await realpath(env.CARGO_NDK_SYSROOT_PATH);
  if (!sysroot.startsWith(ndk + '/') || !(await realpath(clang)).startsWith(ndk + '/'))
    throw new Error('NDK compiler/sysroot is outside its installation');
  const properties = join(ndk, 'source.properties');
  if (!(await stat(properties)).isFile()) throw new Error('NDK source.properties is missing');
  const files = [context.cargo, context.ndk, context.proxy, process.execPath, clang, cmake, properties];
  for (const [name, value] of Object.entries(env))
    if (/^(?:CC|CXX|AR|RANLIB)(?:$|_|-)/.test(name) ||
        /^CARGO_TARGET_.*_(?:AR|LINKER|RUNNER)$/.test(name)) {
      if (!isAbsolute(value) || !(await stat(value)).isFile())
        throw new Error('NDK tool selection must name an existing absolute executable');
      files.push(value);
    }
  const names = new Set(environmentNames);
  for (const name of Object.keys(env))
    if (/^(?:_?CARGO_NDK_|ANDROID_|CARGO_TARGET_|CC[_-]|CXX[_-]|AR[_-]|RANLIB[_-]|CFLAGS[_-]|CXXFLAGS[_-]|LDFLAGS[_-]|BINDGEN_EXTRA_CLANG_ARGS)/.test(name))
      names.add(name);
  return { files: [...new Set(files)], environment: [...names].sort(), ndk };
}

async function readContext(path) {
  if (!path || !isAbsolute(path)) throw new Error('Missing private Android Cargo context');
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const info = await file.stat();
    if (!info.isFile() || info.uid !== process.getuid() || info.nlink !== 1 || info.mode & 0o077)
      throw new Error('Android Cargo context is not an owned private regular file');
    const value = JSON.parse(await file.readFile('utf8'));
    const keys = [
      'schema', 'root', 'target', 'directory', 'cargo', 'ndk', 'proxy', 'manifest', 'package',
      'library', 'abi', 'triple', 'profile', 'api', 'report', 'sourceSha256',
    ];
    if (!value || typeof value !== 'object' || Array.isArray(value) ||
        !same(Object.keys(value).sort(), keys.sort()) || value.schema !== 1 ||
        androidTargets[value.abi] !== value.triple || value.api !== 28 ||
        !['dev', 'release'].includes(value.profile) ||
        typeof value.sourceSha256 !== 'string' || !/^[a-f0-9]{64}$/.test(value.sourceSha256) ||
        typeof value.package !== 'string' || !/^[A-Za-z0-9_-]+$/.test(value.package) ||
        typeof value.library !== 'string' || !/^[A-Za-z0-9_]+$/.test(value.library))
      throw new Error('Invalid Android Cargo context');
    for (const name of ['root', 'target', 'directory', 'cargo', 'ndk', 'proxy', 'manifest', 'report'])
      if (typeof value[name] !== 'string' || !isAbsolute(value[name]))
        throw new Error('Android Cargo context paths must be absolute');
    if (resolve(process.cwd()) !== value.root ||
        !value.manifest.startsWith(value.root + '/') ||
        !value.report.startsWith(value.directory + '/'))
      throw new Error('Android Cargo context does not match this workspace');
    return value;
  } finally {
    await file.close();
  }
}

export async function proxyMain(args = process.argv.slice(2)) {
  const context = await readContext(process.env.ZOEN_ANDROID_CARGO_CONTEXT);
  let active;
  let cancelled = false;
  let claimed = false;
  const cancel = () => {
    cancelled = true;
    active?.kill('SIGTERM');
  };
  if (process.env.ZOEN_ANDROID_CARGO_CONTROL !== '1' || process.stdin.isTTY)
    throw new Error('Android Cargo proxy requires its owned control pipe');
  process.on('SIGTERM', cancel);
  process.on('SIGINT', cancel);
  process.stdin.on('end', cancel);
  process.stdin.on('data', cancel);
  process.stdin.resume();
  const run = (program, arguments_, cwd = context.root, extra = {}) => new Promise((ok, fail) => {
    if (cancelled) return fail(new Error('Android Cargo invocation cancelled'));
    const child = spawn(program, arguments_, {
      cwd, env: { ...process.env, ...extra, CARGO: context.cargo },
      stdio: ['ignore', 'inherit', 'inherit'],
    });
    active = child;
    child.once('error', fail);
    child.once('exit', (code, signal) => {
      active = undefined;
      if (code === 0 && !cancelled) ok();
      else fail(new Error(`Android Cargo child did not complete (${signal ?? code})`));
    });
  });
  try {
    if (same(args, metadataArguments) || args.length === 1 && versionArguments.has(args[0])) {
      try {
        await stat(context.report + '.started');
        throw new Error('cargo-ndk query after its admitted build');
      } catch (error) { if (error.code !== 'ENOENT') throw error; }
      await run(context.cargo, args[0] === 'metadata' ? [...args, '--locked', '--offline'] : args);
      return;
    }
    const build = admittedArguments(context, args);
    // One accepted build per outer NDK invocation, including failed builds.
    const claim = await open(context.report + '.started', 'wx', 0o600);
    await claim.writeFile(JSON.stringify({ pid: process.pid }) + '\n');
    await claim.close();
    claimed = true;
    const inputs = await ndkInputs(context, process.env);
    const guarded = new ExpoCargoArtifacts({
      root: context.root, target: context.target, directory: context.directory,
      run, python: process.env.ZOEN_CARGO_GUARD_PYTHON,
      cargo: context.cargo, inputEnvironment: inputs.environment,
      inputFiles: [...androidCargoTooling.map((name) => join(context.root, 'scripts', name)), ...inputs.files],
    });
    const artifacts = await guarded.build(build, [{
      key: 'library', manifest: context.manifest, name: `lib${context.library}.so`,
    }]);
    if (cancelled) throw new Error('Android Cargo invocation cancelled');
    const qualification = await guarded.finish(context.sourceSha256);
    if (cancelled) throw new Error('Android Cargo invocation cancelled');
    await writeFile(context.report, JSON.stringify({
      schema: 1, abi: context.abi, triple: context.triple, profile: context.profile,
      manifest: context.manifest, library: artifacts.library, qualification, ndk: inputs.ndk,
    }, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
    if (cancelled) throw new Error('Android Cargo invocation cancelled');
  } finally {
    // Per-invocation cleanup acknowledgement, never an artifact qualification.
    // The outer NDK process can die before its owned proxy/guard have retired.
    if (claimed) await writeFile(context.report + '.retired', JSON.stringify({
      pid: process.pid, cancelled,
    }) + '\n', { flag: 'wx', mode: 0o600 });
    process.stdin.pause();
    process.stdin.removeListener('end', cancel);
    process.stdin.removeListener('data', cancel);
    process.stdin.destroy();
    process.removeListener('SIGTERM', cancel);
    process.removeListener('SIGINT', cancel);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  proxyMain().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
