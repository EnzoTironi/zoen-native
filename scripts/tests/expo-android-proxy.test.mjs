import assert from 'node:assert/strict';
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';
import { admittedArguments, ndkArguments, ndkInputs } from '../expo-android-cargo-proxy.mjs';
import { androidOptions } from '../expo-android-guarded.mjs';
import { ExpoCargoArtifacts } from '../expo-cargo-artifacts.mjs';

const context = {
  package: 'actual-core', manifest: '/work/engines/core/Cargo.toml',
  abi: 'arm64-v8a', triple: 'aarch64-linux-android', api: 28, profile: 'release',
};
const emitted = (value = context) => [
  ...ndkArguments(value), '--manifest-path', value.manifest,
  '--target', value.triple, '--message-format', 'json-render-diagnostics',
];

test('only the exact pinned NDK suffix is normalized; semantic inputs cannot be composed away', () => {
  assert.deepEqual(admittedArguments(context, emitted()), [
    '-q', '-p', 'actual-core', '--lib', '--profile', 'release',
    '--target', 'aarch64-linux-android',
  ]);
  for (const args of [
    ['build'], [...emitted(), '--', '-C', 'linker=foreign'],
    ['--config', 'build.target=foreign', ...emitted()],
    emitted().map((arg) => arg === context.manifest ? '/foreign/Cargo.toml' : arg),
    emitted().map((arg) => arg === context.triple ? 'x86_64-linux-android' : arg),
    [...ndkArguments(context), '--target', context.triple, ...emitted().slice(ndkArguments(context).length)],
    emitted().map((arg) => arg === 'json-render-diagnostics' ? 'json' : arg),
    [...emitted(), '--features', 'unreviewed'],
    ['clean', ...emitted().slice(1)],
  ]) assert.throws(() => admittedArguments(context, args), /Unexpected cargo-ndk build/);
});

test('Android options preserve both supported ABIs/profiles and refuse duplicate or unknown targets', () => {
  const value = androidOptions(['--release', '--abis=x86_64,arm64-v8a', '--output=/tmp/owned']);
  assert.equal(value.profile, 'release');
  assert.deepEqual(value.abis, ['x86_64', 'arm64-v8a']);
  assert.equal(value.output, '/tmp/owned');
  for (const args of [['--abis='], ['--abis=arm64-v8a,arm64-v8a'], ['--abis=armeabi-v7a'], ['--target=foreign']])
    assert.throws(() => androidOptions(args));
});

async function tools(t) {
  const root = await mkdtemp(join(tmpdir(), 'ndk-environment-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const files = ['cargo', 'cargo-ndk', 'cargo-ndk-runner', 'proxy', 'ndk/source.properties',
    'ndk/build/cmake/android.toolchain.cmake', 'ndk/bin/clang', 'ndk/bin/clang++', 'ndk/bin/llvm-ar'];
  for (const name of files) {
    const path = join(root, name);
    await mkdir(join(path, '..'), { recursive: true });
    await writeFile(path, name);
  }
  await mkdir(join(root, 'ndk/sysroot'), { recursive: true });
  const value = { ...context, cargo: join(root, 'cargo'), ndk: join(root, 'cargo-ndk'), proxy: join(root, 'proxy') };
  const env = {
    ANDROID_ABI: 'arm64-v8a', ANDROID_PLATFORM: '28',
    _CARGO_NDK_LINK_TARGET: '--target=aarch64-linux-android28',
    _CARGO_NDK_LINK_CLANG: join(root, 'ndk/bin/clang'),
    _CARGO_NDK_LDFLAGS: '-Wl,-z,max-page-size=16384',
    CARGO_NDK_ANDROID_PLATFORM: 'arm64-v8a', // Actual 4.1.2 value, not an inferred API number.
    CARGO_NDK_CMAKE_TOOLCHAIN_PATH: join(root, 'ndk/build/cmake/android.toolchain.cmake'),
    CARGO_NDK_SYSROOT_PATH: join(root, 'ndk/sysroot'),
    CLANG_PATH: join(root, 'ndk/bin/clang'),
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER: value.ndk,
    CARGO_TARGET_AARCH64_LINUX_ANDROID_RUNNER: join(root, 'cargo-ndk-runner'),
    CARGO_TARGET_AARCH64_LINUX_ANDROID_AR: join(root, 'ndk/bin/llvm-ar'),
    CC_aarch64_linux_android: join(root, 'ndk/bin/clang'),
    CXX: join(root, 'ndk/bin/clang++'),
    BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android: '--sysroot=selected',
    CFLAGS_aarch64_linux_android: '--target=aarch64-linux-android28 -DTEST=1',
    PRIVATE_TEST_SECRET: 'never-a-declared-build-input',
  };
  return { root, value, env };
}

test('NDK tool/API/sysroot selection and effective build environment are declared without a raw environment dump', async (t) => {
  const { root, value, env } = await tools(t);
  const inputs = await ndkInputs(value, env);
  assert.equal(inputs.ndk, join(root, 'ndk'));
  for (const name of Object.keys(env).filter((name) => name !== 'PRIVATE_TEST_SECRET'))
    assert(inputs.environment.includes(name), name);
  assert(!inputs.environment.includes('PRIVATE_TEST_SECRET'));
  for (const path of [value.cargo, value.ndk, value.proxy, join(root, 'ndk/bin/clang++'),
    join(root, 'ndk/source.properties'), env.CARGO_NDK_CMAKE_TOOLCHAIN_PATH])
    assert(inputs.files.includes(path), path);
  assert(!JSON.stringify(inputs).includes('never-a-declared-build-input'));
  for (const alteration of [
    { ANDROID_PLATFORM: '29' }, { ANDROID_ABI: 'x86_64' },
    { _CARGO_NDK_LINK_TARGET: '--target=aarch64-linux-android29' },
    { CARGO_NDK_OUTPUT_PATH: '/unowned-output' },
    { CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER: value.cargo },
    { CARGO_NDK_SYSROOT_PATH: root },
  ]) await assert.rejects(ndkInputs(value, { ...env, ...alteration }));
});

test('artifact helper forwards real Cargo and added NDK inputs to the canonical guard', async (t) => {
  const { root, value } = await tools(t);
  const target = join(root, 'target');
  const artifact = join(target, 'libprobe.so');
  await mkdir(target);
  await writeFile(artifact, 'qualified-probe');
  const { createHash } = await import('node:crypto');
  const digest = createHash('sha256').update('qualified-probe').digest('hex');
  let invocation;
  const consumer = new ExpoCargoArtifacts({
    root, target, directory: join(root, 'private'), cargo: value.cargo,
    inputEnvironment: ['_CARGO_NDK_LINK_TARGET'], inputFiles: [value.ndk],
    run: async (command, args) => {
      invocation = args;
      const receipt = args[args.indexOf('--receipt') + 1];
      await writeFile(receipt, JSON.stringify({ status: 'PASS', target,
        before: { packages: {}, context: { config: {} } },
        artifacts: [{ manifest: join(root, 'Cargo.toml'), path: artifact, sha256: digest }],
      }));
    },
  });
  const result = await consumer.build(['--lib'], [{ key: 'probe', manifest: 'Cargo.toml', name: 'libprobe.so' }]);
  assert.equal(invocation[invocation.indexOf('--cargo') + 1], value.cargo);
  assert(invocation.includes('_CARGO_NDK_LINK_TARGET'));
  assert(invocation.includes(value.ndk));
  assert(!invocation.includes('--adopt-idle-target'));
  await chmod(artifact, 0o600);
  await writeFile(artifact, 'replacement');
  assert.equal(await readFile(result.probe, 'utf8'), 'qualified-probe');
});
