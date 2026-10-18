#!/usr/bin/env node
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildApp } from './build.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const [app, resourceFolder = join(root, 'crates/roda-apps/resources')] = process.argv.slice(2);
assert.ok(app, 'Usage: check.mjs <miniapp-directory> [embedded-resource-directory]');
const temporary = await mkdtemp(join(tmpdir(), 'zoen-miniapp-check-'));
try {
  const manifest = await buildApp(resolve(app), temporary);
  const htmlName = `${manifest.id}.html`;
  const manifestName = `${manifest.id}.manifest.json`;
  const [generated, embedded, committedManifest] = await Promise.all([
    readFile(join(temporary, htmlName)),
    readFile(join(resourceFolder, htmlName)),
    readFile(join(resourceFolder, manifestName), 'utf8'),
  ]);
  assert.ok(
    generated.equals(embedded),
    `${htmlName} differs from its source. Rebuild it with tools/miniapp-build/build.mjs.`,
  );
  assert.deepEqual(
    JSON.parse(committedManifest),
    manifest,
    `${manifestName} differs from its source, capabilities or pinned dependencies.`,
  );
  console.log(`Verified embedded ${manifest.id}: ${manifest.bytes} bytes / ${manifest.sha256}`);
} finally {
  // Only the unique temporary build directory created above is removed.
  await rm(temporary, { recursive: true, force: true });
}
