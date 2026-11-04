import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

test(
  'actual iOS Markdown parser emits local hashtag entities without mutating source or code',
  { skip: process.platform !== 'darwin', timeout: 90_000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-hashtag-native-'));
    try {
      const pkg = preparePackage(app, directory);
      mkdirSync(join(directory, 'React'));
      writeFileSync(
        join(directory, 'React/RCTLog.h'),
        '#define RCTLogError(...) NSLog(__VA_ARGS__)\n',
      );
      const run = (args: string[]) =>
        execFileSync('xcrun', ['--sdk', 'macosx', ...args], { encoding: 'utf8', timeout: 60_000 });
      run(['clang', '-c', join(pkg, 'cpp/md4c/md4c.c'), '-o', join(directory, 'md4c.o')]);
      run([
        'clang',
        '-fobjc-arc',
        '-c',
        join(pkg, 'ios/parser/MarkdownASTNode.m'),
        '-o',
        join(directory, 'ast.o'),
      ]);
      run([
        'clang++',
        '-std=c++17',
        '-fobjc-arc',
        '-fblocks',
        '-I' + directory,
        '-I' + join(pkg, 'cpp/parser'),
        '-I' + join(pkg, 'ios/parser'),
        join(app, 'modules/zoen-core/tests/MessageHashtags/HashtagTests.mm'),
        join(pkg, 'ios/parser/MarkdownParserBridge.mm'),
        join(pkg, 'ios/parser/ENRMMarkdownParser.mm'),
        join(directory, 'ast.o'),
        join(pkg, 'cpp/parser/MD4CParser.cpp'),
        join(directory, 'md4c.o'),
        '-framework',
        'Foundation',
        '-o',
        join(directory, 'hashtags'),
      ]);
      assert.match(
        execFileSync(join(directory, 'hashtags'), { encoding: 'utf8' }),
        /Native hashtag AST, literal text and source offsets PASS/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
