import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

test(
  'production Swift assistant context rejects changed sources while admitting unrelated arrivals',
  { skip: process.platform !== 'darwin', timeout: 180_000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-assistant-context-'));
    const binary = join(directory, 'context-tests');
    try {
      execFileSync(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-parse-as-library',
          fileURLToPath(
            new URL(
              '../../modules/zoen-core/ios/ZoenAssistantContextSnapshot.swift',
              import.meta.url,
            ),
          ),
          fileURLToPath(
            new URL('../../modules/zoen-core/tests/AssistantContextTests.swift', import.meta.url),
          ),
          '-o',
          binary,
        ],
        { timeout: 120_000, encoding: 'utf8' },
      );
      assert.match(
        execFileSync(binary, [], { timeout: 10_000, encoding: 'utf8' }),
        /PASS native assistant context/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
