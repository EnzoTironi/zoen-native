import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, cpSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

test(
  'actual native paste preserves literal search syntax across plain/rich handoffs',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-composer-paste-'));
    try {
      const targetInput = join(directory, 'node_modules/react-native-enriched-markdown/ios/input');
      mkdirSync(join(targetInput, '..'), { recursive: true });
      cpSync(join(app, 'node_modules/react-native-enriched-markdown/ios/input'), targetInput, {
        recursive: true,
      });
      const pkg = preparePackage(app, directory);
      const input = join(pkg, 'ios/input');
      const source = readFileSync(
        process.env.ZOEN_COMPOSER_PASTE_SOURCE ?? join(input, 'EnrichedMarkdownTextInput.mm'),
        'utf8',
      );
      const start = source.indexOf('- (void)pasteMarkdown:(NSString *)markdown\n');
      const end = source.indexOf('\n#pragma mark - Formatting', start);
      assert.ok(start > 0 && end > start, 'execute the exact maintained production method');
      writeFileSync(join(directory, 'PasteMethod.inc'), source.slice(start, end));
      const action = readFileSync(
        process.env.ZOEN_COMPOSER_PASTE_ACTION_SOURCE ?? join(input, 'ENRMInputTextView.mm'),
        'utf8',
      );
      const actionStart = action.indexOf('- (void)paste:(id)sender\n');
      const actionEnd = action.indexOf('\n- (BOOL)canPerformAction:', actionStart);
      assert.ok(actionStart > 0 && actionEnd > actionStart);
      writeFileSync(join(directory, 'PasteAction.inc'), action.slice(actionStart, actionEnd));
      const currentStart = source.indexOf('- (NSString *)currentMarkdown\n');
      const currentEnd = source.indexOf(
        '\n- (NSArray<ENRMFormattingRange *> *)allRangesIncludingTransient',
        currentStart,
      );
      assert.ok(currentStart > 0 && currentEnd > currentStart);
      writeFileSync(join(directory, 'CurrentMethod.inc'), source.slice(currentStart, currentEnd));
      execFileSync('xcrun', [
        'clang',
        '-c',
        join(pkg, 'cpp/md4c/md4c.c'),
        '-o',
        join(directory, 'md4c.o'),
      ]);
      const names = [
        'ENRMInputParser',
        'ENRMInputRemend',
        'ENRMInputStyledRange',
        'ENRMFormattingRange',
        'ENRMBlockRange',
        'ENRMMarkdownSerializer',
        'ENRMMarkdownSerializer+Composer',
        'ENRMFormattingStore',
        'ENRMRangeEditAdjustment',
        'ENRMBlockStore',
      ];
      execFileSync('xcrun', [
        'clang',
        '-fobjc-arc',
        '-fblocks',
        '-I' + input,
        '-c',
        join(input, 'ENRMComposerInputState.m'),
        '-o',
        join(directory, 'state.o'),
      ]);
      const binary = join(directory, 'paste');
      execFileSync(
        'xcrun',
        [
          'clang++',
          '-std=c++17',
          '-fobjc-arc',
          '-fblocks',
          '-I' + input,
          '-I' + directory,
          '-I' + join(pkg, 'cpp/md4c'),
          join(app, 'modules/zoen-core/tests/ComposerMarkdown/PasteTests.mm'),
          ...names.map((name) => join(input, name + '.mm')),
          join(directory, 'state.o'),
          join(directory, 'md4c.o'),
          '-framework',
          'Foundation',
          '-o',
          binary,
        ],
        { timeout: 45_000 },
      );
      assert.match(execFileSync(binary, { encoding: 'utf8' }), /Actual native paste method PASS/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
