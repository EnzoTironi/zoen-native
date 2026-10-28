import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

function body(source: string, marker: string) {
  const start = source.indexOf(marker);
  assert.ok(start >= 0, marker);
  const open = source.indexOf('{', start);
  let depth = 1,
    end = open + 1;
  while (depth && end < source.length) {
    if (source[end] === '{') depth++;
    if (source[end] === '}') depth--;
    end++;
  }
  assert.equal(depth, 0);
  return source.slice(start, end);
}

test(
  'maintained AST/copy and actual TextKit layout govern quote previews without changing source',
  { skip: process.platform !== 'darwin', timeout: 90000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-quote-layout-'));
    try {
      const pkg = preparePackage(app, directory);
      writeFileSync(
        join(directory, 'quote-bridge.inc'),
        body(
          readFileSync(join(pkg, 'ios/parser/MarkdownParserBridge.mm'), 'utf8'),
          'static MarkdownASTNode *convertCppASTToObjC(',
        ),
      );
      const code = readFileSync(join(pkg, 'ios/utils/ENRMCodeBlockContent.mm'), 'utf8');
      writeFileSync(
        join(directory, 'quote-code-content.inc'),
        [
          'static void ENRMAppendNodeContent(',
          'NSString *ENRMCodeBlockExtractCode(',
          'NSString *ENRMCodeBlockLanguage(',
          'NSString *ENRMCodeBlockFenceChar(',
          'NSString *ENRMCodeBlockFencedMarkdown(',
        ]
          .map((marker) => body(code, marker))
          .join('\n'),
      );
      const run = (args: string[]) =>
        execFileSync('xcrun', ['--sdk', 'macosx', ...args], { timeout: 60000 });
      run(['clang', '-c', join(pkg, 'cpp/md4c/md4c.c'), '-o', join(directory, 'md4c.o')]);
      for (const [source, name] of [
        ['ios/parser/MarkdownASTNode.m', 'ast'],
        ['ios/utils/MarkdownASTSerializer.m', 'serializer'],
        ['ios/segments/RenderedMarkdownSegment.m', 'segments'],
      ])
        run([
          'clang',
          '-fobjc-arc',
          '-fblocks',
          '-I' + join(pkg, 'ios/parser'),
          '-I' + join(pkg, 'ios/utils'),
          '-I' + join(pkg, 'ios/segments'),
          '-c',
          join(pkg, source!),
          '-o',
          join(directory, name! + '.o'),
        ]);
      const binary = join(directory, 'quote-layout');
      run([
        'clang++',
        '-std=c++17',
        '-fobjc-arc',
        '-fblocks',
        '-I' + directory,
        '-I' + join(pkg, 'ios/parser'),
        '-I' + join(pkg, 'ios/utils'),
        '-I' + join(pkg, 'ios/segments'),
        '-I' + join(pkg, 'cpp/parser'),
        join(app, 'modules/zoen-core/tests/MessageQuotes/QuotePresentationTests.mm'),
        join(pkg, 'ios/utils/ENRMQuotePresentation.mm'),
        join(directory, 'ast.o'),
        join(directory, 'serializer.o'),
        join(directory, 'segments.o'),
        join(pkg, 'cpp/parser/MD4CParser.cpp'),
        join(directory, 'md4c.o'),
        '-framework',
        'Foundation',
        '-framework',
        'AppKit',
        '-o',
        binary,
      ]);
      assert.match(
        execFileSync(binary, { encoding: 'utf8' }),
        /Quote AST, copy, TextKit preview and reading anchor PASS/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);

test(
  'actual retained quote layout restores accessibility after reflow and forced full selection',
  { skip: process.platform !== 'darwin', timeout: 90000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-quote-reflow-'));
    try {
      const pkg = preparePackage(app, directory);
      const method = body(
        readFileSync(join(pkg, 'ios/segments/ENRMBlockquoteContainerView.mm'), 'utf8'),
        '- (void)layoutSubviews {',
      );
      writeFileSync(join(directory, 'quote-layout.inc'), method);
      const binary = join(directory, 'quote-reflow');
      execFileSync('xcrun', [
        '--sdk',
        'macosx',
        'clang',
        '-fobjc-arc',
        '-fblocks',
        '-I' + directory,
        join(app, 'modules/zoen-core/tests/MessageQuotes/QuoteReflowTests.m'),
        '-framework',
        'AppKit',
        '-o',
        binary,
      ]);
      assert.match(execFileSync(binary, { encoding: 'utf8' }), /Retained quote reflow PASS/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
