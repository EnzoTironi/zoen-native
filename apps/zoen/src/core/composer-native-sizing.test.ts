import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

test(
  'maintained composer measurement retains empty and trailing lines at the configured font and width',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-composer-sizing-'));
    try {
      const pkg = preparePackage(app, directory);
      const source = readFileSync(join(pkg, 'ios/input/EnrichedMarkdownTextInput.mm'), 'utf8');
      const methods = ['- (CGSize)measureSize:', '- (NSNumber *)zoenComposerFittingHeight'].map(
        (signature) => {
          const start = source.indexOf(signature);
          const end = source.indexOf('\n}', start);
          assert.ok(start > 0 && end > start, `execute the production ${signature} method`);
          return source.slice(start, end + 2);
        },
      );
      writeFileSync(join(directory, 'SizingMethods.inc'), methods.join('\n'));
      const binary = join(directory, 'sizing');
      execFileSync(
        'xcrun',
        [
          '--sdk',
          'macosx',
          'clang',
          '-fobjc-arc',
          '-Werror',
          '-I' + directory,
          join(app, 'modules/zoen-core/tests/ComposerMarkdown/SizingTests.m'),
          '-framework',
          'AppKit',
          '-o',
          binary,
        ],
        { timeout: 30_000 },
      );
      assert.match(execFileSync(binary, { encoding: 'utf8' }), /Maintained composer sizing PASS/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
