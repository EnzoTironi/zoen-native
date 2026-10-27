import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

test(
  'native composer receipts and undo preserve ownership, canonical preimages and UTF16 selection',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-composer-receipt-'));
    try {
      const pkg = preparePackage(app, directory);
      const binary = join(directory, 'receipt');
      execFileSync(
        'xcrun',
        [
          '--sdk',
          'macosx',
          'clang',
          '-fobjc-arc',
          '-fblocks',
          '-I' + join(pkg, 'ios/input'),
          join(app, 'modules/zoen-core/tests/ComposerMarkdown/InputStateTests.m'),
          join(pkg, 'ios/input/ENRMComposerInputState.m'),
          '-framework',
          'Foundation',
          '-o',
          binary,
        ],
        { timeout: 30_000 },
      );
      assert.match(execFileSync(binary, { encoding: 'utf8' }), /Composer native receipt PASS/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
