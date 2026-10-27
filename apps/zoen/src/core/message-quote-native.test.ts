import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
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
    const installed = join(app, 'node_modules/react-native-enriched-markdown');
    try {
      const patch = join(
        app,
        'patches/react-native-enriched-markdown+1.1.1+007+quote-source-map.patch',
      );
      const paths = [
        ...readFileSync(patch, 'utf8').matchAll(
          /^diff --git a\/node_modules\/react-native-enriched-markdown\/(.+) b\//gm,
        ),
      ].map((match) => match[1]!);
      for (const path of paths) {
        if (!existsSync(join(installed, path))) continue;
        mkdirSync(join(pkg, path, '..'), { recursive: true });
        cpSync(join(installed, path), join(pkg, path));
      }
      for (const path of ['cpp/parser', 'cpp/md4c'])
        cpSync(join(installed, path), join(pkg, path), { recursive: true });
      if (!existsSync(join(pkg, 'ios/utils/ENRMQuoteRanges.m')))
        execFileSync('patch', ['--batch', '--forward', '-p1', '-i', patch], {
          cwd: directory,
        });
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
