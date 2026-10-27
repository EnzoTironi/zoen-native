import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

test(
  'actual image recipe restores framing and orientation without losing recent ink pixels',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-photo-reset-'));
    const native = fileURLToPath(new URL('../../modules/zoen-core/', import.meta.url));
    const binary = join(directory, 'pixels');
    try {
      execFileSync(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-module-cache-path',
          join(directory, 'module-cache'),
          join(native, 'ios/ZoenPhotoEditorImage.swift'),
          join(native, 'tests/PhotoEditorImageTests.swift'),
          '-o',
          binary,
        ],
        { timeout: 45_000 },
      );
      assert.match(
        execFileSync(binary, { encoding: 'utf8', timeout: 5_000 }),
        /Photo editor pixels PASS/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
