import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  cp,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rm,
  stat,
  utimes,
  writeFile,
} from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { promisify } from 'node:util';
import { cargoTooling, ExpoCargoArtifacts } from '../expo-cargo-artifacts.mjs';

const execute = promisify(execFile);
const source = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const python = process.env.ZOEN_CARGO_GUARD_PYTHON ?? 'python3';
const cargo = process.env.CARGO_TEST_BIN ?? join(process.env.HOME, '.cargo/bin/cargo');

async function write(path, text) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, text);
}

async function fixture(t) {
  const base = process.env.EXPO_CARGO_TEST_ROOT ?? tmpdir();
  await mkdir(base, { recursive: true });
  const path = await mkdtemp(join(base, 'expo-cargo-'));
  if (!process.env.EXPO_CARGO_TEST_ROOT) t.after(() => rm(path, { recursive: true, force: true }));
  return realpath(path);
}

async function workspace(path, marker) {
  for (const name of cargoTooling) {
    await mkdir(join(path, 'scripts'), { recursive: true });
    const from =
      name === 'build-expo-core.mjs' && process.env.EXPO_BUILDER_UNDER_TEST
        ? process.env.EXPO_BUILDER_UNDER_TEST
        : join(source, 'scripts', name);
    await cp(from, join(path, 'scripts', name));
  }
  await write(
    join(path, 'Cargo.toml'),
    '[workspace]\nmembers=["crates/client-core"]\nresolver="2"\n',
  );
  await write(
    join(path, 'crates/client-core/Cargo.toml'),
    '[package]\nname="client-core"\nversion="0.1.0"\nedition="2021"\n[features]\nclient-types=[]\n',
  );
  await write(
    join(path, 'crates/client-core/examples/export-client-types.rs'),
    `fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    std::fs::create_dir_all(&out).unwrap();
    for name in ["Command.ts", "Snapshot.ts"] {
      std::fs::write(out.join(name), concat!("${marker}|", env!("CARGO_MANIFEST_DIR"))).unwrap();
    }
  }\n`,
  );
  await write(
    join(path, '.gitignore'),
    '/target/\n/apps/zoen/modules/zoen-core/generated/\n/apps/zoen/src/core/generated/\n',
  );
  await execute(cargo, ['generate-lockfile', '--offline'], { cwd: path });
  await execute('git', ['init', '-q', path]);
  await execute('git', ['add', '.'], { cwd: path });
  // Reproduce Cargo's same-name, old-mtime worktree collision, not a fresh-source
  // control which happens to force recompilation without the guard.
  async function age(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      if (entry.name === '.git') continue;
      const item = join(directory, entry.name);
      if (entry.isDirectory()) await age(item);
      else await utimes(item, 1_000_000_000, 1_000_000_000);
    }
  }
  await age(path);
}

async function build(path, target, extra = {}) {
  const env = {
    ...process.env,
    CARGO_BUILD_JOBS: '1',
    CARGO_PROFILE_TEST_DEBUG: '0',
    ZOEN_CARGO_GUARD_PYTHON: python,
    ...extra,
  };
  delete env.CARGO_TARGET_DIR;
  delete env.CARGO_BUILD_BUILD_DIR;
  if (target !== undefined) env.CARGO_TARGET_DIR = target;
  return execute(process.execPath, ['scripts/build-expo-core.mjs', 'types'], {
    cwd: path,
    env,
    maxBuffer: 8 * 1024 * 1024,
    timeout: 60_000,
  });
}

async function stamp(path) {
  return JSON.parse(
    await readFile(join(path, 'apps/zoen/modules/zoen-core/generated/types.json'), 'utf8'),
  );
}

test('actual types entrypoint binds old-mtime A → B shared builds and runs private qualified exporters', async (t) => {
  const path = await fixture(t);
  const a = join(path, 'a');
  const b = join(path, 'b');
  const target = join(path, 'shared');
  await workspace(a, 'A');
  await workspace(b, 'B');
  await build(a, target);
  const before = await stamp(a);
  await build(b, target);
  for (const name of ['Command.ts', 'Snapshot.ts']) {
    const output = await readFile(join(b, 'apps/zoen/src/core/generated', name), 'utf8');
    assert.equal(output, `B|${join(b, 'crates/client-core')}`);
  }
  const qa = JSON.parse(await readFile(before.cargoQualification, 'utf8'));
  const copy = qa.builds[0].artifacts[0];
  assert.match(copy.path, /artifacts-1\/exporter\/export-client-types$/);
  assert.equal((await stat(copy.path)).mode & 0o777, 0o500);
  const qb = JSON.parse(await readFile((await stamp(b)).cargoQualification, 'utf8'));
  const guard = JSON.parse(await readFile(qb.builds[0].receipt, 'utf8'));
  assert.equal(guard.status, 'PASS');
  assert(guard.cleanedPackages.length > 0);
  assert(guard.artifacts.every((artifact) => artifact.manifest.startsWith(b + '/')));
  for (const artifact of qb.builds[0].artifacts) {
    assert.equal(
      createHash('sha256')
        .update(await readFile(artifact.path))
        .digest('hex'),
      artifact.sha256,
    );
  }
  // A's copy is still A after B has replaced the shared Cargo output.
  const out = join(path, 'replay-a');
  await execute(copy.path, [out]);
  assert.equal(
    await readFile(join(out, 'Command.ts'), 'utf8'),
    `A|${join(a, 'crates/client-core')}`,
  );
});

test('unknown nonempty target fails without adoption or overwriting prior generated output', async (t) => {
  const path = await fixture(t);
  const root = join(path, 'workspace');
  const target = join(path, 'unknown');
  await workspace(root, 'refused');
  await write(join(target, 'keep'), 'unmanaged');
  const output = join(root, 'apps/zoen/src/core/generated/Command.ts');
  await write(output, 'previous qualified output');
  await assert.rejects(build(root, target), (error) => {
    assert.match(error.stdout + error.stderr, /Unmanaged nonempty target/);
    assert.match(error.stderr, /explicit idle adoption/);
    assert.match(error.stderr, /No automatic adoption/);
    return true;
  });
  assert.equal(await readFile(output, 'utf8'), 'previous qualified output');
  assert.equal(await readFile(join(target, 'keep'), 'utf8'), 'unmanaged');
  await assert.rejects(stamp(root), { code: 'ENOENT' });
});

test('default isolated types path still uses its existing Cargo run contract', async (t) => {
  const path = await fixture(t);
  await workspace(path, 'isolated');
  await build(path);
  assert.equal((await stamp(path)).cargoQualification, undefined);
  assert.equal(
    await readFile(join(path, 'apps/zoen/src/core/generated/Command.ts'), 'utf8'),
    `isolated|${join(path, 'crates/client-core')}`,
  );
});

test('artifact consumer rejects raced bytes, missing artifacts and changed source between builds', async (t) => {
  const root = await fixture(t);
  const target = join(root, 'target');
  const artifact = join(target, 'debug/tool');
  await write(artifact, 'good');
  const digest = createHash('sha256').update('good').digest('hex');
  let mode = 'good';
  const commands = [];
  const guarded = new ExpoCargoArtifacts({
    root,
    target,
    directory: join(root, 'receipts'),
    run: async (command, args, cwd, environment) => {
      commands.push({ command, args, cwd, environment });
      const receipt = args[args.indexOf('--receipt') + 1];
      await writeFile(
        receipt,
        JSON.stringify({
          status: 'PASS',
          target,
          before: {
            packages: { probe: mode === 'changed-source' ? 'new' : 'original' },
            context: { config: {} },
          },
          artifacts:
            mode === 'missing'
              ? []
              : [{ manifest: join(root, 'Cargo.toml'), path: artifact, sha256: digest }],
        }),
      );
      if (mode === 'raced') await writeFile(artifact, 'replaced');
    },
  });
  const specification = [{ key: 'tool', manifest: 'Cargo.toml', name: 'tool' }];
  await guarded.build(['-p', 'probe'], specification, { SDKROOT: '/sdk/host' });
  assert.equal(commands[0].environment.SDKROOT, '/sdk/host');
  assert(commands[0].args.includes('ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON'));
  assert(!commands[0].args.includes('--adopt-idle-target'));
  mode = 'changed-source';
  await assert.rejects(guarded.build([], specification), /source changed between/);
  mode = 'missing';
  await assert.rejects(guarded.build([], specification), /Expected one qualified/);
  mode = 'raced';
  await assert.rejects(guarded.build([], specification), /changed before copying/);
});
