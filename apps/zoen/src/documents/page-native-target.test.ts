import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

test(
  'actual Swift Page target decoder keeps whole-document input distinct from legacy append and block',
  { skip: process.platform !== 'darwin', timeout: 60000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-page-target-'));
    const root = new URL('../../modules/zoen-core/', import.meta.url);
    try {
      const binary = join(directory, 'request-tests');
      execFileSync(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-j',
          '1',
          '-module-cache-path',
          join(directory, 'cache'),
          fileURLToPath(new URL('ios/ZoenPageInputOpenRequest.swift', root)),
          fileURLToPath(new URL('tests/PageInputOpenRequestTests.swift', root)),
          '-o',
          binary,
        ],
        { encoding: 'utf8', timeout: 40000 },
      );
      assert.match(
        execFileSync(binary, [], { encoding: 'utf8', timeout: 10000 }),
        /PASS native Page target normalization, legacy compatibility, whole-document isolation and strict shapes/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
