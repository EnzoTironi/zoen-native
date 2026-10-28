import { t } from '../i18n/index.ts';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { preparePackage } from './message-markdown-native.fixtures.ts';
import { selectedMessageQuote } from './selected-message-quote.ts';
import type { TimelineEntry } from './types';

function body(source: string, marker: string) {
  const start = source.indexOf(marker);
  assert.ok(start >= 0);
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
  'actual GitHub selector consumes the actual segmented native callback with original Unicode source anchors',
  { skip: process.platform !== 'darwin', timeout: 90000 },
  async () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-github-quote-'));
    try {
      const pkg = preparePackage(app, directory);
      const native = readFileSync(join(pkg, 'ios/EnrichedMarkdown.mm'), 'utf8');
      writeFileSync(
        join(directory, 'github-bridge.inc'),
        body(
          readFileSync(join(pkg, 'ios/parser/MarkdownParserBridge.mm'), 'utf8'),
          'static MarkdownASTNode *convertCppASTToObjC(',
        ),
      );
      writeFileSync(
        join(directory, 'github-segments.inc'),
        body(
          readFileSync(join(pkg, 'ios/segments/SegmentRenderer.m'), 'utf8'),
          'static NSArray *ENRMSplitASTIntoSegments(',
        ),
      );
      writeFileSync(
        join(directory, 'github-text-render.inc'),
        body(
          readFileSync(join(pkg, 'ios/renderer/TextRenderer.m'), 'utf8'),
          '- (void)renderNodeContent:',
        ),
      );

      const implementation = native.slice(native.indexOf('@implementation EnrichedMarkdown'));
      writeFileSync(
        join(directory, 'github-emitter.inc'),
        body(implementation, '- (void)emitContextMenuItemPress:'),
      );
      const delegate = implementation.slice(implementation.indexOf('editMenuForTextInRange:'));
      writeFileSync(
        join(directory, 'github-callback.inc'),
        body(delegate, '^(NSString *itemText,'),
      );
      const run = (args: string[]) =>
        execFileSync('xcrun', ['--sdk', 'macosx', ...args], { timeout: 60000, encoding: 'utf8' });
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
      for (const [name, relative] of [
        ['ast', 'ios/parser/MarkdownASTNode.m'],
        ['segments', 'ios/segments/RenderedMarkdownSegment.m'],
      ])
        run([
          'clang',
          '-fobjc-arc',
          '-I' + join(pkg, 'ios/parser'),
          '-I' + join(pkg, 'ios/segments'),
          '-c',
          join(pkg, relative!),
          '-o',
          join(directory, name + '.o'),
        ]);
      run([
        'clang++',
        '-std=c++17',
        '-fobjc-arc',
        '-fblocks',
        '-I' + directory,
        '-I' + join(pkg, 'cpp/parser'),
        '-I' + join(pkg, 'ios/utils'),
        '-I' + join(pkg, 'ios/parser'),
        '-I' + join(pkg, 'ios/segments'),
        join(app, 'modules/zoen-core/tests/MessageQuote/GitHubCallbackTests.mm'),
        join(pkg, 'cpp/parser/MD4CParser.cpp'),
        join(directory, 'ast.o'),
        join(directory, 'segments.o'),
        join(directory, 'map.o'),
        join(directory, 'md4c.o'),
        '-framework',
        'Foundation',
        '-o',
        join(directory, 'callback'),
      ]);
      const event = JSON.parse(
        execFileSync(join(directory, 'callback'), [t('chat.quoteSelection')], { encoding: 'utf8' }),
      );
      const invoked: string[] = [];
      let githubProps: Record<string, any> | undefined;
      const compiled = await build({
        entryPoints: [join(app, 'src/components/message-markdown.tsx')],
        bundle: true,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'actual-github-native-ports',
            setup(builder) {
              builder.onResolve({ filter: /^\.\.\/i18n\/index\.ts$/ }, ({ path }) => ({
                path,
                external: true,
              }));
              builder.onResolve({ filter: /^react-native-enriched-markdown$/ }, () => ({
                path: join(pkg, 'src/native/EnrichedMarkdownText.tsx'),
              }));
              builder.onResolve(
                {
                  filter:
                    /^(\.\.\/theme|\.\/use-link-launch|\.\/use-message-code-copy|\.\/use-quote-collapse)$/,
                },
                ({ path }) => ({ path, external: true }),
              );

              builder.onResolve(
                { filter: /^(react|react-native|react\/jsx-runtime)$/ },
                ({ path }) => ({ path, external: true }),
              );
              builder.onResolve(
                { filter: /EnrichedMarkdown(?:Text)?NativeComponent$/ },
                ({ path }) => ({ path, external: true }),
              );
            },
          },
        ],
      });
      const module = { exports: {} as { MessageMarkdown(props: any): unknown } };
      runInNewContext(compiled.outputFiles[0]!.text, {
        module,
        exports: module.exports,
        __DEV__: false,
        process: { env: { EXPO_OS: 'ios' } },
        require(name: string) {
          if (name === '../i18n/index.ts') return { t };
          if (name === 'react')
            return {
              memo: (fn: unknown) => fn,
              createContext: () => ({}),
              use: () => null,
              useRef: (value: unknown) => ({ current: value }),
              useMemo: (fn: () => unknown) => fn(),
              useCallback: (fn: unknown) => fn,
              useEffect: (fn: () => unknown) => fn(),
            };
          if (name === '../theme')
            return {
              useTheme: () => ({
                scheme: 'light',
                colors: {
                  text: '#111111',
                  secondary: '#555555',
                  accent: '#0066cc',
                  surface: '#ffffff',
                  line: '#777777',
                  muted: '#eeeeee',
                  danger: '#cc0000',
                },
              }),
            };
          if (name === './use-link-launch') return { useLinkLaunch: () => () => {} };
          if (name === './use-quote-collapse') return { useQuoteCollapse: () => ({}) };
          if (name === './use-message-code-copy') return { useMessageCodeCopy: () => () => {} };
          if (name === 'react-native')
            return {
              Platform: { OS: 'ios', select: (values: any) => values.ios ?? values.default },
              processColor: (value: unknown) => value,
            };
          if (name === 'react/jsx-runtime')
            return { jsx: (type: (props: unknown) => unknown, props: unknown) => type(props) };
          if (name.endsWith('/EnrichedMarkdownNativeComponent'))
            return (props: any) => {
              invoked.push('github');
              githubProps = props;
            };
          if (name.endsWith('/EnrichedMarkdownTextNativeComponent'))
            return () => assert.fail('Message quotes must exercise the actual GitHub renderer');
          throw new Error(`Unexpected native port: ${name}`);
        },
      });
      const source = '😀 same\n\n```txt\nskip\n```\n\n😀 **same**';
      const entry = {
        id: 'source',
        text_revision: 'signed-revision',
        deleted: false,
        kind: { Message: { text: source, card: null } },
      } as unknown as TimelineEntry;
      let selected: unknown;
      module.exports.MessageMarkdown({
        text: source,
        color: '#111111',
        fontSize: 17,
        maxWidth: 300,
        selectable: true,
        onQuoteSelection: (text: string, ranges: string) => {
          selected = selectedMessageQuote(entry, text, ranges);
        },
      });

      assert.deepEqual(invoked, ['github']);
      assert.ok(githubProps);
      githubProps.onContextMenuItemPress({ nativeEvent: event });
      const start = source.lastIndexOf('same');
      assert.deepEqual(JSON.parse(JSON.stringify(selected)), {
        source_id: 'source',
        revision: 'signed-revision',
        text: 'same',
        spans: [{ start_utf16: start, end_utf16: start + 4 }],
      });
      assert.equal(event.selectionStart, 3);
      assert.notEqual(start, 3);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
