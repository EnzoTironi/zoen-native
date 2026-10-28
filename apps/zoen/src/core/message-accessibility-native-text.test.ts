import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

function implementation(source: string, signature: string) {
  const start = source.lastIndexOf(signature);
  assert.ok(start >= 0, signature);
  const open = source.indexOf('{', start);
  let end = open + 1,
    depth = 1;
  while (depth && end < source.length) {
    if (source[end] === '{') depth++;
    if (source[end] === '}') depth--;
    end++;
  }
  assert.equal(depth, 0);
  return source.slice(start, end);
}

test(
  'accessibility text comes from the maintained native GFM AST projection, retaining literal punctuation and links',
  { skip: process.platform !== 'darwin', timeout: 60000 },
  () => {
    const pkg = fileURLToPath(
      new URL('../../node_modules/react-native-enriched-markdown/', import.meta.url),
    );
    const directory = mkdtempSync(join(tmpdir(), 'zoen-accessibility-text-'));
    try {
      const serializer = readFileSync(join(pkg, 'ios/utils/MarkdownASTSerializer.m'), 'utf8');
      const bridge = readFileSync(join(pkg, 'ios/parser/MarkdownParserBridge.mm'), 'utf8');
      writeFileSync(
        join(directory, 'projection.mm'),
        `
#import <Foundation/Foundation.h>
#import "MarkdownASTNode.h"
#include "MD4CParser.hpp"
${implementation(bridge, 'static MarkdownASTNode *convertCppASTToObjC(')}
${implementation(serializer, 'static void appendPlainText(')}
${implementation(serializer, 'NSString *plainTextFromASTNode(')}
static void links(MarkdownASTNode *node, NSMutableArray *result) {
  if (node.type == MarkdownNodeTypeLink) [result addObject:node.attributes[@"url"] ?: @""];
  for (MarkdownASTNode *child in node.children) links(child, result);
}
int main() { @autoreleasepool {
  NSData *input = [[NSFileHandle fileHandleWithStandardInput] readDataToEndOfFile];
  NSString *markdown = [[NSString alloc] initWithData:input encoding:NSUTF8StringEncoding];
  Markdown::MD4CParser parser; Markdown::Md4cFlags flags;
  flags.hardSoftBreaks = true; flags.preserveBlankLines = true;
  flags.latexMath = false; flags.noMedia = true;
  MarkdownASTNode *ast = convertCppASTToObjC(parser.parse(markdown.UTF8String, flags, true));
  NSMutableArray *urls = [NSMutableArray new]; links(ast, urls);
  NSData *output = [NSJSONSerialization dataWithJSONObject:@{@"text":plainTextFromASTNode(ast), @"links":urls} options:0 error:nil];
  [[NSFileHandle fileHandleWithStandardOutput] writeData:output];
} }
`,
      );
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
        '-I' + join(pkg, 'ios/parser'),
        '-c',
        join(pkg, 'ios/parser/MarkdownASTNode.m'),
        '-o',
        join(directory, 'ast.o'),
      ]);
      const binary = join(directory, 'projection');
      execFileSync(
        'xcrun',
        [
          'clang++',
          '-std=c++17',
          '-fobjc-arc',
          '-fblocks',
          '-I' + join(pkg, 'ios/parser'),
          '-I' + join(pkg, 'cpp/parser'),
          '-I' + join(pkg, 'cpp/md4c'),
          join(directory, 'projection.mm'),
          join(directory, 'ast.o'),
          join(pkg, 'cpp/parser/MD4CParser.cpp'),
          join(directory, 'md4c.o'),
          '-framework',
          'Foundation',
          '-o',
          binary,
        ],
        { timeout: 45000 },
      );
      const parse = (markdown: string): { text: string; links: string[] } =>
        JSON.parse(execFileSync(binary, { input: markdown, encoding: 'utf8' }));
      assert.equal(
        parse(String.raw`QA753\: nota em Mensagens Salvas\.`).text,
        'QA753: nota em Mensagens Salvas.',
      );
      assert.equal(
        parse('\\*literal\\* \\[a\\]\\(b\\) \\`z\\` café🦊').text,
        '*literal* [a](b) `z` café🦊',
      );
      const rich = parse(
        '**bold** *italic* ~~strike~~\n\n> quoted\n\n[label](https://example.test/path)',
      );
      for (const word of ['bold', 'italic', 'strike', 'quoted', 'label'])
        assert.ok(rich.text.includes(word));
      assert.deepEqual(rich.links, ['https://example.test/path']);
      assert.equal(parse('`a*b [x](y)`').text, 'a*b [x](y)');
      assert.equal(parse('```text\na*b [x](y)\n```').text, 'a*b [x](y)');
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
