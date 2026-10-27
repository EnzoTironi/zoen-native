import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

test(
  'actual native Site preview allows one initial document and retires navigation access',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-site-preview-'));
    try {
      const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
        encoding: 'utf8',
        timeout: 10_000,
      }).trim();
      const files = [
        'ios/Sites/ZoenSitePreviewPolicy.swift',
        'tests/SitePreviewPolicyTests.swift',
      ].map((file) => fileURLToPath(new URL(`../../modules/zoen-core/${file}`, import.meta.url)));
      const binary = join(directory, 'probe');
      execFileSync(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          // Compile both actual files in one frontend, rather than loading
          // Foundation in separate primary-file jobs with a cold module cache.
          // This keeps -Onone assertions, strict flags and the 40-second bound.
          '-whole-module-optimization',
          '-module-cache-path',
          join(directory, 'module-cache'),
          ...files,
          '-o',
          binary,
        ],
        { env: { ...process.env, SDKROOT: sdk }, timeout: 40_000 },
      );
      assert.match(
        execFileSync(binary, [], { encoding: 'utf8', timeout: 10_000 }),
        /PASS actual Site preview initial-only navigation, retirement and restrictive prefix/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
