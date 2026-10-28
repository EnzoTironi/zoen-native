import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

test(
  'native style updates resize plain and rich runs, deferring full refresh through composition',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-composer-style-'));
    try {
      // Copy the upstream formatter and handlers, then apply missing maintained
      // patches through the same fixture used by the other native contract tests.
      const pkg = join(directory, 'node_modules/react-native-enriched-markdown');
      cpSync(
        join(app, 'node_modules/react-native-enriched-markdown/ios/input'),
        join(pkg, 'ios/input'),
        {
          recursive: true,
        },
      );
      preparePackage(app, directory);
      const input = join(pkg, 'ios/input');
      const source = readFileSync(join(input, 'EnrichedMarkdownTextInput.mm'), 'utf8');
      const methods = [
        '- (void)resetBaseTypingAttributes\n',
        '- (void)applyFormatting\n',
        '- (void)applyFormattingScopedToRange:',
      ].map((signature) => {
        const start = source.indexOf(signature);
        const end = source.indexOf('\n}', start);
        assert.ok(start >= 0 && end > start, `execute production ${signature}`);
        return source.slice(start, end + 2);
      });
      const styleUpdate = source.match(
        /  if \(styleChanged\) \{([\s\S]*?)\n  \} else if \(writingDirectionChanged/,
      );
      assert.ok(styleUpdate, 'execute actual updateProps style branch');
      writeFileSync(join(directory, 'StyleMethods.inc'), methods.join('\n'));
      writeFileSync(join(directory, 'StyleUpdate.inc'), styleUpdate[1]!);
      const tests = join(app, 'modules/zoen-core/tests/ComposerMarkdown');
      cpSync(join(tests, 'StyleTestPlatform.h'), join(directory, 'ENRMUIKit.h'));
      const utilities = join(app, 'node_modules/react-native-enriched-markdown/ios/utils');
      cpSync(join(utilities, 'FontUtils.h'), join(directory, 'FontUtils.h'));
      const fontUtils = readFileSync(join(utilities, 'FontUtils.m'), 'utf8');
      const weight = fontUtils.indexOf('UIFontWeight ENRMFontWeightFromString');
      assert.ok(weight >= 0);
      writeFileSync(
        join(directory, 'FontWeight.mm'),
        '#import "FontUtils.h"\n' + fontUtils.slice(weight),
      );
      const files = [
        'ENRMInputFormatter',
        'ENRMInputFormatterStyle',
        'ENRMFormattingRange',
        'ENRMBlockRange',
        'ENRMInputStyledRange',
        'ENRMInputBlockType',
        'ENRMEditSession',
      ];
      const binary = join(directory, 'style');
      execFileSync(
        'xcrun',
        [
          '--sdk',
          'macosx',
          'clang++',
          '-std=c++17',
          '-fobjc-arc',
          '-fblocks',
          '-I' + directory,
          '-I' + input,
          '-I' + join(input, 'styles'),
          join(tests, 'StyleTests.mm'),
          join(directory, 'FontWeight.mm'),
          ...files.map((name) => join(input, name + '.mm')),
          ...readdirSync(join(input, 'styles'))
            .filter((name) => name.endsWith('.mm'))
            .map((name) => join(input, 'styles', name)),
          '-framework',
          'AppKit',
          '-framework',
          'QuartzCore',
          '-o',
          binary,
        ],
        { timeout: 45_000 },
      );
      assert.match(execFileSync(binary, { encoding: 'utf8' }), /Maintained composer style PASS/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
