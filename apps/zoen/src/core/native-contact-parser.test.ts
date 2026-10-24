import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

// Real Apple Contacts/CryptoKit and production parser; no fake RFC parser.
test(
  'actual native vCard parser preserves reviewed fields and rejects malformed, multi-contact, oversize and mismatched bytes',
  { skip: process.platform !== 'darwin', timeout: 60000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-contact-parser-'));
    try {
      const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
        encoding: 'utf8',
        timeout: 10000,
      }).trim();
      const source = fileURLToPath(
        new URL('../../modules/zoen-core/ios/ZoenContactPreview.swift', import.meta.url),
      );
      const tests = fileURLToPath(
        new URL('../../modules/zoen-core/tests/ContactPreviewTests.swift', import.meta.url),
      );
      const binary = join(directory, 'probe');
      execFileSync(
        'xcrun',
        [
          '--sdk',
          'macosx',
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-module-cache-path',
          join(directory, 'module-cache'),
          '-sdk',
          sdk,
          source,
          tests,
          '-o',
          binary,
        ],
        { encoding: 'utf8', timeout: 45000 },
      );
      assert.match(
        execFileSync(binary, [], { encoding: 'utf8', timeout: 10000 }),
        /Contact parser PASS/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
