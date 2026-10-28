import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

test(
  'actual MD4C parser, native stores and serializer retain composer Markdown through edits',
  { skip: process.platform !== 'darwin', timeout: 90_000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-composer-markdown-'));
    const pkg = join(directory, 'node_modules/react-native-enriched-markdown');
    const input = join(pkg, 'ios/input');
    const installed = join(app, 'node_modules/react-native-enriched-markdown');
    try {
      for (const path of ['ios/input', 'cpp/md4c']) {
        mkdirSync(join(pkg, path, '..'), { recursive: true });
        cpSync(join(installed, path), join(pkg, path), { recursive: true });
      }
      const patches = [
        ['ENRMInputParser.mm', 'parseComposerMarkdown:', '010+composer-parser'],
        ['ENRMMarkdownSerializer+Composer.mm', 'serializeComposerText:', '012+composer-formatting'],
        [
          'ENRMComposerRanges.mm',
          'ENRMNormalizeComposerRanges',
          '018+composer-range-normalization',
        ],
        [
          'ENRMComposerVariants.mm',
          'ENRMRepresentableComposerInline',
          '019+composer-inline-variants',
        ],
        [
          'ENRMComposerAvailability.mm',
          'ENRMComposerCanToggle',
          '021+composer-format-availability',
        ],
      ];
      for (const [source, marker, name] of patches) {
        if (
          existsSync(join(input, source!)) &&
          readFileSync(join(input, source!), 'utf8').includes(marker!)
        )
          continue;
        execFileSync(
          'patch',
          [
            '--batch',
            '--forward',
            '-p1',
            '-i',
            join(app, `patches/react-native-enriched-markdown+1.1.1+${name}.patch`),
          ],
          { cwd: directory },
        );
      }
      execFileSync('xcrun', [
        'clang',
        '-c',
        join(pkg, 'cpp/md4c/md4c.c'),
        '-o',
        join(directory, 'md4c.o'),
      ]);
      execFileSync('xcrun', [
        'clang',
        '-fobjc-arc',
        '-fblocks',
        '-c',
        '-I' + input,
        join(input, 'ENRMComposerInputState.m'),
        '-o',
        join(directory, 'input-state.o'),
      ]);
      const binary = join(directory, 'roundtrip');
      const files = [
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
        'ENRMComposerRanges',
        'ENRMComposerVariants',
        'ENRMComposerAvailability',
        'ENRMBlockEditCoordinator',
        'ENRMLinkCoordinator',
      ];
      execFileSync(
        'xcrun',
        [
          'clang++',
          '-std=c++17',
          '-fobjc-arc',
          '-fblocks',
          '-I' + input,
          '-I' + join(pkg, 'cpp/md4c'),
          join(app, 'modules/zoen-core/tests/ComposerMarkdown/RoundTripTests.mm'),
          ...files.map((name) => join(input, `${name}.mm`)),
          join(directory, 'input-state.o'),
          join(directory, 'md4c.o'),
          '-framework',
          'Foundation',
          '-o',
          binary,
        ],
        { timeout: 60_000 },
      );
      type Result = {
        plain: string;
        markdown: string;
        types: number[];
        ranges: { type: number; start: number; length: number }[];
        selection: number[];
        available: boolean;
      };
      const run = (markdown: string, options: Record<string, unknown> = {}): Result =>
        JSON.parse(
          execFileSync(binary, {
            input: JSON.stringify({ markdown, ...options }),
            encoding: 'utf8',
          }),
        );
      for (const [markdown, plain] of [
        ['**forte**', 'forte'],
        ['_ênfase_', 'ênfase'],
        ['~~corte~~', 'corte'],
        ['||segredo||', 'segredo'],
        ['***ambos***', 'ambos'],
        ['`a`', 'a'],
        ['`  a  `', ' a '],
        ['[😀 link](https://example.org/a_(b))', '😀 link'],
        ['> **citação**\n> `código`', 'citação\ncódigo'],
      ]) {
        const first = run(markdown!);
        assert.equal(first.plain, plain);
        assert.equal(run(first.markdown).plain, plain);
        assert.deepEqual(run(first.markdown).types, first.types);
      }
      assert.deepEqual(run('_ênfase_').types, [1], 'underscore is emphasis, never false underline');
      assert.deepEqual(
        run('_ênfase_', { generic: true }).types,
        [2],
        'generic article parser remains unchanged',
      );
      const opaque = [
        '| __a__ | b |\n| --- | --- |\n| \\| | y |',
        '- [x] __task__',
        '![__alt__](https://example.org/a.png)',
        '[ref][id]\n\n[id]: https://example.org',
        '<span>__raw__</span>',
        '> outer\n>> inner',
        '[]()',
        '**unfinished',
        '[a](https://example.org/a\\(b\\))',
        '`a\nb`',
        '![ ](https://example.org/a.png)',
        '||||',
        '```md\n__raw__ \\*x\\*\n```',
        '[label](https://example.org/a_(b) "title (paren)")',
      ];
      for (const source of opaque) {
        assert.equal(run(source).markdown, source, 'unsupported source must stay byte-identical');
        const edited = run(source, { edit: { start: 0, delete: 0, text: 'prefix ' } });
        assert.equal(edited.markdown, 'prefix ' + source);
      }
      const escaped = '\\*literal\\*';
      assert.equal(run(escaped).markdown, escaped);
      assert.equal(
        run(escaped, { edit: { start: 0, delete: 0, text: '😀 ' } }).markdown,
        '😀 ' + escaped,
      );
      assert.equal(
        run(escaped, { edit: { start: 1, delete: 1, text: 'L' } }).markdown,
        '\\*Literal\\*',
      );
      assert.equal(run('`ab`', { edit: { start: 1, delete: 0, text: '`' } }).markdown, '``a`b``');
      assert.equal(run('ab', { code: [0, 2] }).markdown, '`ab`');
      assert.equal(run('a\nb', { code: [0, 3] }).markdown, '`a`\n`b`');
      assert.equal(run('a\nb', { quote: [0, 3] }).markdown, '> a\n> b');
      const crossed = run('**ab**cd', { code: [1, 2], selection: [1, 2] });
      assert.equal(crossed.plain, 'abcd');
      assert.equal(crossed.markdown, '**a`b`**`c`d');
      assert.deepEqual(crossed.selection, [1, 2]);
      const coverage = (result: Result) =>
        Array.from({ length: result.plain.length }, (_, i) =>
          [
            ...new Set(
              result.ranges
                .filter((r) => i >= r.start && i < r.start + r.length)
                .map((r) => r.type),
            ),
          ].sort(),
        );
      const reopened = run(crossed.markdown);
      assert.equal(reopened.plain, crossed.plain);
      assert.deepEqual(coverage(reopened), coverage(crossed));
      assert.equal(reopened.markdown, crossed.markdown);
      const undone = run('**ab**cd', { code: [1, 2], selection: [1, 2], undo: true });
      assert.equal(undone.markdown, '**ab**cd');
      assert.deepEqual(undone.selection, [1, 2]);
      const unicodeCode = run('**😀a**bc', { code: [2, 2], selection: [2, 2] });
      assert.equal(unicodeCode.markdown, '**😀`a`**`b`c');
      assert.equal(run(unicodeCode.markdown).plain, '😀abc');
      assert.deepEqual(unicodeCode.selection, [2, 2]);
      const crossedEmphasis = run('abcde', {
        toggles: [
          { type: 0, start: 0, length: 3 },
          { type: 1, start: 2, length: 3 },
        ],
      });
      assert.deepEqual(coverage(crossedEmphasis), [[0], [0], [0, 1], [1], [1]]);
      assert.equal(crossedEmphasis.markdown, '**ab*c***_de_');
      assert.equal(run(crossedEmphasis.markdown).markdown, crossedEmphasis.markdown);
      for (const toggles of [
        [
          { type: 0, start: 0, length: 3 },
          { type: 1, start: 2, length: 3 },
        ],
        [
          { type: 1, start: 0, length: 3 },
          { type: 0, start: 2, length: 3 },
        ],
        [
          { type: 0, start: 0, length: 3 },
          { type: 3, start: 2, length: 3 },
        ],
        [
          { type: 3, start: 0, length: 3 },
          { type: 0, start: 2, length: 3 },
        ],
        [
          { type: 0, start: 0, length: 3 },
          { type: 5, start: 2, length: 3 },
        ],
      ]) {
        const styled = run('abcde', { toggles });
        const reimported = run(styled.markdown);
        assert.equal(reimported.plain, 'abcde');
        assert.deepEqual(coverage(reimported), coverage(styled));
        assert.equal(run(reimported.markdown).plain, 'abcde');
      }
      const escapedCode = run('\\*literal\\*', { code: [0, 9] });
      assert.equal(run(escapedCode.markdown).plain, '*literal*');
      assert.equal(
        run('\\*literal\\*', { code: [0, 9], toggles: [{ type: 6, start: 0, length: 9 }] })
          .markdown,
        '\\*literal\\*',
      );
      assert.equal(
        run('`*literal*`', { toggles: [{ type: 6, start: 0, length: 9 }] }).markdown,
        '\\*literal\\*',
      );
      for (const value of ['*x*', '[a](b)', '`c`', '\\*literal\\*', '🪷 café']) {
        const edited = run('**café** 🦊', { edit: { start: 2, delete: 0, text: value } });
        const expected = 'ca' + value + 'fé 🦊';
        assert.equal(edited.plain, expected);
        assert.equal(run(edited.markdown).plain, expected);
        assert.equal(run(run(edited.markdown).markdown).plain, expected);
        const undoEdit = run('**café** 🦊', {
          edit: { start: 2, delete: 0, text: value },
          undo: true,
        });
        assert.equal(undoEdit.markdown, '**café** 🦊');
      }
      // Plausible long draft under the same native fixture deadline. Strategy
      // count remains four; it never branches once per formatting range.
      const longText = 'café 🦊 abcdefghij '.repeat(40);
      const longToggles = Array.from({ length: 40 }, (_, i) => [
        { type: 0, start: i * 19, length: 10 },
        { type: 1, start: i * 19 + 5, length: 12 },
      ]).flat();
      const longDraft = run(longText, { toggles: longToggles });
      assert.equal(longDraft.plain, longText);
      assert.equal(run(longDraft.markdown).plain, longText);
      for (const markdown of ['<span>raw</span>', '| a | b |\n| --- | --- |\n| c | d |']) {
        const partial = run(markdown, { availability: { type: 6, start: 2, length: 2 } });
        assert.equal(partial.available, false);
        assert.equal(partial.markdown, markdown, 'availability probe cannot mutate opaque source');
        assert.equal(
          run(markdown, { availability: { type: 6, start: 0, length: markdown.length } }).available,
          true,
        );
      }
      assert.equal(
        run('<span>raw</span>', { availability: { quote: true, start: 2, length: 2 } }).available,
        false,
      );
      const linked = '[label](https://original.example)';
      assert.equal(
        run(linked, { availability: { type: 4, start: 0, length: 2, url: 'https://new.example' } })
          .markdown,
        linked,
      );
      assert.equal(
        run('abcde', { availability: { type: 0, start: 0, length: 3 } }).available,
        true,
      );
      assert.equal(
        run('abcde', { availability: { quote: true, start: 0, length: 3 } }).available,
        true,
      );
      assert.equal(
        run('abcde', { availability: { type: 4, start: 0, length: 3 } }).available,
        true,
      );
      assert.equal(run('  ', { availability: { type: 0, start: 0, length: 2 } }).available, false);
      const unformatted = run('abc ', {
        toggles: [
          { type: 0, start: 0, length: 4 },
          { type: 0, start: 0, length: 4 },
        ],
        selection: [0, 4],
      });
      assert.equal(unformatted.markdown, 'abc ');
      assert.deepEqual(unformatted.selection, [0, 4]);
      assert.equal(
        run('**before**\n\n' + opaque[0] + '\n\n_after_').plain,
        'before\n\n' + opaque[0] + '\n\nafter',
        'opaque block cannot erase surrounding rich spans',
      );
      const production = readFileSync(join(input, 'EnrichedMarkdownTextInput.mm'), 'utf8');
      const actionStart = production.indexOf('- (dispatch_block_t)composerMenuAction:');
      const actionEnd = production.indexOf('\n- (void)performComposerFormatting:', actionStart);
      assert.ok(actionStart >= 0 && actionEnd > actionStart);
      writeFileSync(
        join(directory, 'ComposerMenuAction.inc'),
        production.slice(actionStart, actionEnd),
      );
      const menuBinary = join(directory, 'menu-action');
      execFileSync('xcrun', [
        'clang',
        '-fobjc-arc',
        '-fblocks',
        '-I' + input,
        '-I' + directory,
        join(app, 'modules/zoen-core/tests/ComposerMarkdown/MenuActionTests.m'),
        join(input, 'ENRMComposerInputState.m'),
        '-framework',
        'Foundation',
        '-o',
        menuBinary,
      ]);
      assert.match(
        execFileSync(menuBinary, { encoding: 'utf8' }),
        /Production composer menu action PASS/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
