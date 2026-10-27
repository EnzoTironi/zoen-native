import { preparePackage } from './message-markdown-native.fixtures.ts';
import { createRequire } from 'node:module';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

// Qualify the maintained patch privately; installed/shared dependencies are read-only.
test(
  'actual MD4C callbacks + Foundation quote mapper preserve literal spans and reject layout gaps',
  { skip: process.platform !== 'darwin', timeout: 90000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-quote-native-'));
    const pkg = join(directory, 'node_modules/react-native-enriched-markdown');
    try {
      preparePackage(app, directory);
      const run = (args: string[]) =>
        execFileSync('xcrun', ['--sdk', 'macosx', ...args], {
          timeout: 60000,
          encoding: 'utf8',
        });
      run(['clang', '-c', join(pkg, 'cpp/md4c/md4c.c'), '-o', join(directory, 'md4c.o')]);
      run([
        'clang',
        '-fobjc-arc',
        '-fblocks',
        '-c',
        join(pkg, 'ios/utils/ENRMQuoteRanges.m'),
        '-o',
        join(directory, 'map.o'),
      ]);
      run([
        'clang++',
        '-std=c++17',
        '-fobjc-arc',
        '-I' + join(pkg, 'cpp/parser'),
        '-I' + join(pkg, 'ios/utils'),
        join(app, 'modules/zoen-core/tests/MessageQuote/SourceMapTests.mm'),
        join(pkg, 'cpp/parser/MD4CParser.cpp'),
        join(directory, 'map.o'),
        join(directory, 'md4c.o'),
        '-framework',
        'Foundation',
        '-o',
        join(directory, 'quote'),
      ]);
      assert.match(
        execFileSync(join(directory, 'quote'), { encoding: 'utf8' }),
        /Native quote source map PASS/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);

test(
  'packaged quote event emitters equal genuine codegen with the pod-local include layout',
  { timeout: 60000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-quote-codegen-'));
    try {
      const pkg = preparePackage(app, directory);
      const schema = join(directory, 'schema.json');
      execFileSync(
        process.execPath,
        [
          join(
            app,
            'node_modules/@react-native/codegen/lib/cli/combine/combine-js-to-schema-cli.js',
          ),
          schema,
          '--platform',
          'ios',
          join(pkg, 'src'),
        ],
        { timeout: 30000 },
      );
      const require = createRequire(import.meta.url);
      const codegen = require('@react-native/codegen/lib/generators/RNCodegen.js');
      const output = join(directory, 'generated');
      codegen.generate(
        {
          libraryName: 'EnrichedMarkdownTextSpec',
          schema: JSON.parse(readFileSync(schema, 'utf8')),
          outputDirectory: output,
          packageName: 'com.swmansion.enriched.markdown',
          assumeNonnull: false,
          useLocalIncludePaths: true,
        },
        { generators: ['events'] },
      );
      for (const file of ['EventEmitters.h', 'EventEmitters.cpp'])
        assert.equal(
          readFileSync(
            join(pkg, 'ios/generated/ReactCodegen/EnrichedMarkdownTextSpec', file),
            'utf8',
          ),
          readFileSync(join(output, file), 'utf8'),
          file + ' must match actual pod-local codegen',
        );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
