import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

test(
  'actual Swift scroll metrics preserve no-op reads, ordering, coalescing and retirement',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-scroll-metrics-'));
    try {
      const binary = join(directory, 'probe');
      const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
        encoding: 'utf8',
        timeout: 10_000,
      }).trim();
      const files = ['ios/ZoenScrollMetricsState.swift', 'tests/ScrollMetricsTests.swift'].map(
        (file) => fileURLToPath(new URL(`../../modules/zoen-core/${file}`, import.meta.url)),
      );
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
          ...files,
          '-o',
          binary,
        ],
        { env: { ...process.env, SDKROOT: sdk }, timeout: 40_000 },
      );
      const result = execFileSync(binary, [], { encoding: 'utf8', timeout: 10_000 });
      assert.match(
        result,
        /PASS native scroll metrics ordering, no-op reads, coalescing, retirement and overflow/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
