import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

function implementation(source: string, marker: string) {
  const start = source.indexOf(marker);
  assert.ok(start >= 0, marker);
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
  'public semantic projection uses the maintained full native parser and serializer',
  { skip: process.platform !== 'darwin', timeout: 90000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-semantic-preview-'));
    try {
      const pkg = preparePackage(app, directory);
      // Only the React logging sink is a port; the parser bridge compiles unchanged.
      mkdirSync(join(directory, 'React'));
      writeFileSync(join(directory, 'React/RCTLog.h'), '#define RCTLogError(...) ((void)0)\n');
      const code = readFileSync(join(pkg, 'ios/utils/ENRMCodeBlockContent.mm'), 'utf8');
      writeFileSync(
        join(directory, 'projection.mm'),
        `
#import <Foundation/Foundation.h>
#import "ENRMPlainTextProjection.h"
#import "ENRMCodeBlockContent.h"
#import "MarkdownASTNode.h"
${[
  'static void ENRMAppendNodeContent(',
  'NSString *ENRMCodeBlockExtractCode(',
  'NSString *ENRMCodeBlockLanguage(',
  'NSString *ENRMCodeBlockFenceChar(',
  'NSString *ENRMCodeBlockFencedMarkdown(',
]
  .map((marker) => implementation(code, marker))
  .join('\n')}
int main() { @autoreleasepool {
  NSData *data = [[NSFileHandle fileHandleWithStandardInput] readDataToEndOfFile];
  NSString *markdown = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
  NSString *text = ENRMProjectPlainText(markdown);
  if (!text) return 2;
  NSData *result = [NSJSONSerialization dataWithJSONObject:@[text] options:0 error:nil];
  [[NSFileHandle fileHandleWithStandardOutput] writeData:result];
} }
`,
      );
      const run = (args: string[]) =>
        execFileSync('xcrun', ['--sdk', 'macosx', ...args], { timeout: 60000 });
      run(['clang', '-c', join(pkg, 'cpp/md4c/md4c.c'), '-o', join(directory, 'md4c.o')]);
      const objects = [];
      for (const [source, name] of [
        ['ios/parser/MarkdownASTNode.m', 'ast'],
        ['ios/utils/MarkdownASTSerializer.m', 'serializer'],
      ]) {
        const object = join(directory, name! + '.o');
        run([
          'clang',
          '-fobjc-arc',
          '-fblocks',
          '-I' + join(pkg, 'ios/parser'),
          '-I' + join(pkg, 'ios/utils'),
          '-c',
          join(pkg, source!),
          '-o',
          object,
        ]);
        objects.push(object);
      }
      const binary = join(directory, 'projection');
      run([
        'clang++',
        '-std=c++17',
        '-fobjc-arc',
        '-fblocks',
        '-I' + directory,
        '-I' + join(pkg, 'ios/parser'),
        '-I' + join(pkg, 'ios/utils'),
        '-I' + join(pkg, 'cpp/parser'),
        join(directory, 'projection.mm'),
        join(pkg, 'ios/utils/ENRMPlainTextProjection.mm'),
        join(pkg, 'ios/parser/ENRMMarkdownParser.mm'),
        join(pkg, 'ios/parser/MarkdownParserBridge.mm'),
        join(pkg, 'cpp/parser/MD4CParser.cpp'),
        ...objects,
        join(directory, 'md4c.o'),
        '-framework',
        'Foundation',
        '-o',
        binary,
      ]);
      const project = (markdown: string): string =>
        JSON.parse(execFileSync(binary, { input: markdown, encoding: 'utf8' }))[0];
      assert.equal(project(String.raw`\"Conversa pronta\"`), '"Conversa pronta"');
      assert.equal(
        project('\\*literal\\* \\[a\\]\\(b\\) \\`z\\` café🦊'),
        '*literal* [a](b) `z` café🦊',
      );
      assert.equal(
        project('**Olá** *café* ~~erro~~ [link](https://example.test/a_(b))'),
        'Olá café erro link',
      );
      assert.equal(project('`a*b [x](y) \\`'), 'a*b [x](y) \\');
      assert.equal(project('```text\na*b [x](y) \\ café🦊\n```'), 'a*b [x](y) \\ café🦊');
      const full = '**' + 'café🦊 '.repeat(40).trim() + '**';
      assert.equal(project(full), 'café🦊 '.repeat(40).trim());
      assert.ok(
        !project(full).includes('**'),
        'project full source before the two-line UI truncation',
      );
      assert.equal(project(''), '');
      assert.equal(
        project('> **café🦊**\n> [Read more](https://example.org/a_(b))'),
        'café🦊\nRead more',
      );
      assert.equal(
        project(String.raw`[\*QA769 café🦊 link\*](https://example.org/a_(b))`),
        '*QA769 café🦊 link*',
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);

test(
  'the public projection spec reproduces its genuine iOS and Android codegen contracts',
  { timeout: 60000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-semantic-codegen-'));
    try {
      const pkg = preparePackage(app, directory);
      const schema = join(directory, 'schema.json');
      const run = (script: string, args: string[]) =>
        execFileSync(process.execPath, [join(app, 'node_modules', script), ...args], {
          timeout: 30000,
        });
      run('@react-native/codegen/lib/cli/combine/combine-js-to-schema-cli.js', [
        schema,
        join(pkg, 'src/NativeEnrichedMarkdownProjection.ts'),
      ]);
      for (const platform of ['ios', 'android']) {
        const output = join(directory, platform);
        run('react-native/scripts/generate-specs-cli.js', [
          '-p',
          platform,
          '-t',
          'modules',
          '-s',
          schema,
          '-o',
          output,
          '-n',
          'EnrichedMarkdownTextSpec',
          '-j',
          'com.swmansion.enriched.markdown',
        ]);
        const files =
          platform === 'ios'
            ? [
                [
                  'EnrichedMarkdownTextSpec/EnrichedMarkdownTextSpec.h',
                  'ios/generated/ReactCodegen/EnrichedMarkdownTextSpec/EnrichedMarkdownTextSpec.h',
                ],
                [
                  'EnrichedMarkdownTextSpec/EnrichedMarkdownTextSpec-generated.mm',
                  'ios/generated/ReactCodegen/EnrichedMarkdownTextSpec/EnrichedMarkdownTextSpec-generated.mm',
                ],
                [
                  'EnrichedMarkdownTextSpecJSI.h',
                  'ios/generated/ReactCodegen/EnrichedMarkdownTextSpecJSI.h',
                ],
              ]
            : [
                [
                  'java/com/swmansion/enriched/markdown/NativeEnrichedMarkdownProjectionSpec.java',
                  'android/generated/java/com/swmansion/enriched/markdown/NativeEnrichedMarkdownProjectionSpec.java',
                ],
                [
                  'jni/EnrichedMarkdownTextSpec.h',
                  'android/generated/jni/EnrichedMarkdownTextSpec.h',
                ],
                [
                  'jni/EnrichedMarkdownTextSpec-generated.cpp',
                  'android/generated/jni/EnrichedMarkdownTextSpec-generated.cpp',
                ],
                [
                  'jni/react/renderer/components/EnrichedMarkdownTextSpec/EnrichedMarkdownTextSpecJSI.h',
                  'android/generated/jni/react/renderer/components/EnrichedMarkdownTextSpec/EnrichedMarkdownTextSpecJSI.h',
                ],
              ];
        for (const [generated, shipped] of files)
          assert.equal(
            readFileSync(join(output, generated!), 'utf8'),
            readFileSync(join(pkg, shipped!), 'utf8'),
            shipped,
          );
      }
      const config = JSON.parse(readFileSync(join(pkg, 'package.json'), 'utf8')).codegenConfig;
      assert.equal(config.type, 'all');
      assert.equal(
        config.ios.modules.EnrichedMarkdownProjection.className,
        'ENRMMarkdownProjectionModule',
      );
      assert.equal(config.includesGeneratedCode, true);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
