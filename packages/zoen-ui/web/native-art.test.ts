import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { existsSync, readFileSync } from 'node:fs';
const output = resolve(import.meta.dirname, '../node_modules/.cache/native-art-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'native-art.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic' });
const { nativeAvatarSource, nativeAvatarAsset } = await import(pathToFileURL(output).href);
const identity = { kind: 'group', name: 'Beach trip', avatarRef: 'group-beach-trip' };
test('native avatar size, motion and fixed-frame rules select the actual native artwork', () => {
  assert.equal(nativeAvatarAsset(identity).name, 'GroupBeachTrip');
  assert.equal(nativeAvatarSource(identity, 55), './assets/avatars/Stills/GroupBeachTrip.png');
  assert.equal(nativeAvatarSource(identity, 56), './assets/avatars/Loops/GroupBeachTrip.webp');
  assert.equal(nativeAvatarSource(identity, 56, { fixed: true }), './assets/avatars/Frame0/GroupBeachTrip.png');
  assert.equal(nativeAvatarSource(identity, 56, { fixed: true, reduceMotion: true }), './assets/avatars/Stills/GroupBeachTrip.png');
  assert.equal(nativeAvatarSource(identity, 64, { animate: false }), './assets/avatars/Stills/GroupBeachTrip.png');
  assert.equal(nativeAvatarSource({ kind: 'person', name: 'Ana', photoUrl: '/photo.png' }, 64), '/photo.png');
  assert.ok(existsSync(resolve(import.meta.dirname, 'assets/native-avatar-frame0/GroupBeachTrip.png')));
  assert.equal(nativeAvatarSource({ kind: 'agent', name: 'Zoen', handle: 'zoen' }, 30), './assets/native/head.png');
});
test('native Ink posters preserve actual phase, renderer frame and exported pixel sizes', () => {
  const directory = resolve(import.meta.dirname, 'assets/native-ink-phase2.4');
  const manifest = JSON.parse(readFileSync(resolve(directory, 'manifest.json'), 'utf8'));
  assert.equal(manifest.complete, true); assert.equal(manifest.phase_seconds, 2.4);
  assert.equal(manifest.ink_frame, 0); assert.equal(manifest.transparent, true);
  for (const file of manifest.files) {
    const data = readFileSync(resolve(directory, file.file));
    assert.equal(data.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
    assert.equal(data.readUInt32BE(16), file.pixels); assert.equal(data.readUInt32BE(20), file.pixels);
  }
});
