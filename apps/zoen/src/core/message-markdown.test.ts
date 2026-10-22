import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { messageMarkdownLink, messageMayContainMarkdown } from './message-markdown.ts';

test('ordinary messages keep measured text while Markdown candidates reach the real parser', () => {
  for (const text of ['Hello there', 'Olá 👋', 'First line\nSecond line', '2 + 2 = 4'])
    assert.equal(messageMayContainMarkdown(text), false, text);
  for (const text of [
    '**bold** and *italic*',
    '__underline__',
    '~~removed~~',
    '`inline()`',
    '```ts\nconst value = 1;\n```',
    '# Heading',
    '> A quotation',
    '- first\n- second',
    '1. one\n2. two',
    '[link](https://example.com)',
    '| Column |\n| --- |\n| Value |',
    '||spoiler||',
  ])
    assert.equal(messageMayContainMarkdown(text), true, text);
});

test('Markdown links admit explicit user navigation but reject executable and local schemes', () => {
  for (const url of [
    'https://example.com/page?value=1#section',
    'http://example.com/',
    'mailto:person@example.com',
    'tel:+5511999999999',
    'zoen://page/example',
  ])
    assert.equal(messageMarkdownLink(url), new URL(url).href);
  for (const url of [
    'javascript:alert(1)',
    'data:text/html,hello',
    'file:///private/local-file',
    'intent://anything',
    '/relative',
    '//example.com',
    ' https://example.com',
    'https://example.com\n/path',
    'https://user:password@example.com',
    'https://example.com/\u0000',
    '',
  ])
    assert.equal(messageMarkdownLink(url), null, url);
});

const source = fileURLToPath(new URL('../components/message-markdown.tsx', import.meta.url));
const nativePorts = new Set([
  'react',
  'react/jsx-runtime',
  'react-native',
  'react-native-enriched-markdown',
  '../theme',
]);
const compiled = await build({
  entryPoints: [source],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'message-markdown-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          nativePorts.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

function fixture() {
  const opened: string[] = [];
  const colors = { accent: '#1478d4', secondary: '#798088', surface: '#232933', line: '#3b424b' };
  const module = { exports: {} as Record<string, unknown> };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    URL,
    require(name: string) {
      if (name === 'react')
        return { memo: (value: unknown) => value, useMemo: (fn: () => unknown) => fn() };
      if (name === 'react/jsx-runtime')
        return { jsx: (type: unknown, props: unknown) => ({ type, props }) };
      if (name === 'react-native')
        return {
          Linking: {
            openURL: (url: string) => {
              opened.push(url);
              return Promise.resolve();
            },
          },
        };
      if (name === 'react-native-enriched-markdown')
        return { EnrichedMarkdownText: 'NativeMarkdownText' };
      if (name === '../theme') return { useTheme: () => ({ colors }) };
      throw new Error(`Unexpected native port ${name}`);
    },
  });
  return {
    opened,
    colors,
    render: module.exports.MessageMarkdown as (props: Record<string, unknown>) => {
      type: unknown;
      props: Record<string, unknown>;
    },
  };
}

test('the native renderer gets the exact signed source with read-only task lists and chat typography', () => {
  const f = fixture();
  const text =
    '# Plano\n\n**Bold** and *italic*\n\n> Quote\n\n- [ ] Task\n\n```rust\nlet s = "**literal**";\n```\n\n| Item | Value |\n| --- | --- |\n| A | 1 |';
  const rendered = f.render({
    text,
    color: '#ffffff',
    fontSize: 17,
    maxWidth: 300,
    testID: 'message-body',
  });
  assert.equal(rendered.type, 'NativeMarkdownText');
  assert.equal(rendered.props.markdown, text);
  assert.equal(rendered.props.flavor, 'github');
  assert.equal(rendered.props.enableTaskListItemToggle, false);
  assert.equal(rendered.props.enableLinkPreview, false);
  assert.equal(rendered.props.selectable, false);
  assert.equal(rendered.props.enableBlockContextMenu, false);
  assert.equal(rendered.props.allowFontScaling, true);
  assert.equal(rendered.props.streamingAnimation, false);
  const style = rendered.props.markdownStyle as Record<string, Record<string, unknown>>;
  assert.equal(style.paragraph!.color, '#ffffff');
  assert.equal(style.paragraph!.fontSize, 17);
  assert.equal(style.codeBlock!.fontFamily, 'Menlo');
  assert.equal(style.link!.color, f.colors.accent);
  assert.equal(style.table!.headerTextColor, '#ffffff');
  const flags = rendered.props.md4cFlags as Record<string, unknown>;
  assert.equal(flags.hardSoftBreaks, true);
  assert.equal(flags.preserveBlankLines, true);
  assert.equal(flags.latexMath, false);
  assert.equal(readFileSync(source, 'utf8').includes('dangerouslySetInnerHTML'), false);
});

test('rendered link events use the navigation boundary, and text selection is an explicit mode', () => {
  const f = fixture();
  const { props } = f.render({
    text: '[text](https://example.com)',
    color: '#fff',
    fontSize: 20,
    maxWidth: 350,
    selectable: true,
  });
  const press = props.onLinkPress as (event: { url: string }) => void;
  press({ url: 'javascript:alert(1)' });
  press({ url: 'file:///private/local-file' });
  assert.deepEqual(f.opened, []);
  press({ url: 'https://example.com/path' });
  assert.deepEqual(f.opened, ['https://example.com/path']);
  assert.equal(props.selectable, true);
  assert.equal(props.enableBlockContextMenu, true);
});
