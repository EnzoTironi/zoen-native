import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
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
          join(directory, 'md4c.o'),
          '-framework',
          'Foundation',
          '-o',
          binary,
        ],
        { timeout: 60_000 },
      );
      type Result = { plain: string; markdown: string; types: number[] };
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
      assert.equal(
        run('**before**\n\n' + opaque[0] + '\n\n_after_').plain,
        'before\n\n' + opaque[0] + '\n\nafter',
        'opaque block cannot erase surrounding rich spans',
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
