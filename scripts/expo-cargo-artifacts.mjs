import { createHash } from 'node:crypto';
import { constants, createReadStream } from 'node:fs';
import { chmod, copyFile, mkdir, mkdtemp, readFile, realpath, writeFile } from 'node:fs/promises';
import { basename, isAbsolute, join, relative, resolve } from 'node:path';

export const cargoTooling = [
  'build-expo-core.mjs',
  'expo-cargo-artifacts.mjs',
  'cargo-source-guard.py',
  'cargo_guard_configuration.py',
];

const defaultInputEnvironment = [
  'SDKROOT',
  'DEVELOPER_DIR',
  'LIBRARY_PATH',
  'DYLD_LIBRARY_PATH',
  'ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON',
];

async function sha256(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

// The guard leases each Cargo invocation. Hash-check private copies afterward;
// reject a concurrent replacement or mixed copy instead of trusting a later path.
// Consumers use these private copies, never a later read of a shared target path.
export class ExpoCargoArtifacts {
  constructor({ root, target, directory, run, python, cargo, inputEnvironment = [], inputFiles = [] }) {
    this.root = root;
    this.target = resolve(root, target);
    this.directory = directory;
    this.run = run;
    this.python = python ?? 'python3';
    this.cargo = cargo;
    this.inputEnvironment = [...new Set([...defaultInputEnvironment, ...inputEnvironment])];
    this.inputFiles = inputFiles;
    this.receipts = [];
  }

  async build(args, artifacts, environment = {}) {
    if (!this.privateDirectory) {
      this.root = await realpath(this.root);
      await mkdir(this.directory, { recursive: true });
      this.privateDirectory = await mkdtemp(join(this.directory, 'build-'));
    }
    const receipt = join(this.privateDirectory, `${this.receipts.length + 1}.json`);
    try {
      await this.run(
        this.python,
        [
          join(this.root, 'scripts/cargo-source-guard.py'),
          '--manifest-path',
          join(this.root, 'Cargo.toml'),
          '--target-dir',
          this.target,
          '--receipt',
          receipt,
          ...(this.cargo ? ['--cargo', this.cargo] : []),
          ...this.inputEnvironment.flatMap((name) => ['--input-env', name]),
          ...[...cargoTooling.map((name) => join(this.root, 'scripts', name)), ...this.inputFiles]
            .flatMap((path) => ['--input-file', path]),
          '--',
          'build',
          ...args,
        ],
        this.root,
        environment,
      );
    } catch (cause) {
      let detail = '';
      try {
        const failure = JSON.parse(await readFile(receipt, 'utf8'));
        if (typeof failure.error === 'string') detail = ` ${failure.error}.`;
      } catch {
        // Python/startup failure may precede receipt creation; retain its cause.
      }
      throw new Error(
        `Guarded Cargo build failed; receipt: ${receipt}.${detail} An unknown nonempty target requires explicit idle adoption with scripts/cargo-source-guard.py --adopt-idle-target (see docs/dev/expo-shared-cargo-target.md). No automatic adoption was attempted.`,
        { cause },
      );
    }
    const qualified = JSON.parse(await readFile(receipt, 'utf8'));
    if (qualified.status !== 'PASS' || qualified.target !== (await realpath(this.target)))
      throw new Error(`Unqualified Cargo receipt: ${receipt}`);
    const sources = JSON.stringify({
      packages: Object.entries(qualified.before.packages).sort(),
      configuration: Object.entries(qualified.before.context.config).sort(),
    });
    if (this.sources && this.sources !== sources)
      throw new Error(`Workspace source changed between guarded builds: ${receipt}`);
    this.sources = sources;
    const result = {};
    const copied = [];
    for (const { key, manifest, name, executable = false } of artifacts) {
      const matches = new Map(
        qualified.artifacts
          .filter(
            (artifact) =>
              artifact.manifest === resolve(this.root, manifest) &&
              basename(artifact.path) === name,
          )
          .map((artifact) => [artifact.path, artifact]),
      );
      if (matches.size !== 1) throw new Error(`Expected one qualified ${name}: ${receipt}`);
      const artifact = [...matches.values()][0];
      const path = await realpath(artifact.path);
      const inside = relative(await realpath(this.target), path);
      if (
        !inside ||
        inside.startsWith('..') ||
        isAbsolute(inside) ||
        !/^[a-f0-9]{64}$/.test(artifact.sha256)
      )
        throw new Error(`Invalid qualified artifact: ${name}`);
      const copyDirectory = join(
        this.privateDirectory,
        `artifacts-${this.receipts.length + 1}`,
        key,
      );
      await mkdir(copyDirectory, { recursive: true });
      const copy = join(copyDirectory, name);
      await copyFile(path, copy, constants.COPYFILE_EXCL);
      if ((await sha256(copy)) !== artifact.sha256)
        throw new Error(`Shared Cargo artifact changed before copying: ${name}`);
      await chmod(copy, executable ? 0o500 : 0o400);
      result[key] = copy;
      copied.push({ path: copy, source: path, sha256: artifact.sha256 });
    }
    this.receipts.push({ receipt, artifacts: copied });
    return result;
  }

  async finish(sourceSha256) {
    const path = join(this.privateDirectory, 'qualification.json');
    await writeFile(
      path,
      `${JSON.stringify({ sourceSha256, target: this.target, builds: this.receipts }, null, 2)}\n`,
      { flag: 'wx', mode: 0o600 },
    );
    return path;
  }
}
