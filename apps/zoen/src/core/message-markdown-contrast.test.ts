import * as React from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { THEME_PROFILES } from '../theme/catalog.ts';
import { DEFAULT_THEME_CUSTOMIZATION } from '../theme/telegram.ts';
import { contrastRatio, mixColor, resolveSkyTheme } from '../theme/sky.ts';

const component = fileURLToPath(new URL('../components/message-markdown.tsx', import.meta.url));
const normalizer = fileURLToPath(
  new URL(
    '../../node_modules/react-native-enriched-markdown/lib/module/normalizeMarkdownStyle.js',
    import.meta.url,
  ),
);
const nativePorts = new Set([
  'react',
  'react/jsx-runtime',
  'react-native',
  'react-native-enriched-markdown',
  '../theme',
  './use-link-launch',
  './use-message-code-copy',
]);
const compiled = await build({
  stdin: {
    contents: `export { MessageMarkdown } from ${JSON.stringify(component)};
export { normalizeMarkdownStyle } from ${JSON.stringify(normalizer)};`,
    resolveDir: fileURLToPath(new URL('.', import.meta.url)),
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'markdown-table-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          nativePorts.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

type NormalizedStyle = Record<
  'table' | 'paragraph' | 'strong' | 'em' | 'blockquote' | 'code',
  Record<string, string>
> & {
  codeBlock: {
    color: string;
    backgroundColor: string;
    syntaxColors: Record<string, string>;
  };
};

function fixture() {
  let theme = resolveSkyTheme('day', new Date(2026, 9, 26, 12));
  const root = createReactTestRoot();
  const themeContext = React.createContext(theme);
  let rendered: Record<string, unknown> | null = null;
  function NativeMarkdownText(props: Record<string, unknown>) {
    React.useLayoutEffect(() => {
      rendered = props;
    }, [props]);
    return null;
  }
  const module = { exports: {} as Record<string, unknown> };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    __DEV__: false,
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/jsx-runtime') return jsxRuntime;
      if (name === 'react-native')
        return {
          Text: 'PlainText',
          Platform: { OS: 'ios', select: (values: Record<string, unknown>) => values.ios },
          // Color transport is the native port; library defaults/merging stay real.
          processColor: (value: string) => value,
        };
      if (name === 'react-native-enriched-markdown')
        return { EnrichedMarkdownText: NativeMarkdownText };
      if (name === '../theme') return { useTheme: () => React.use(themeContext) };
      if (name === './use-link-launch') return { useLinkLaunch: () => () => {} };
      if (name === './use-message-code-copy') return { useMessageCodeCopy: () => () => {} };
      throw new Error(`Unexpected native port ${name}`);
    },
  });
  const Component = module.exports.MessageMarkdown as React.ComponentType<Record<string, unknown>>;
  const normalize = module.exports.normalizeMarkdownStyle as (style: unknown) => NormalizedStyle;
  return {
    close: root.close,
    async render(next: typeof theme, color: string, markdown: string) {
      theme = next;
      await root.render(
        React.createElement(
          themeContext,
          { value: next },
          React.createElement(Component, { text: markdown, color, fontSize: 17, maxWidth: 300 }),
        ),
      );
      const props = rendered as Record<string, unknown> | null;
      assert.ok(props, 'Markdown must reach the real native component port');
      assert.equal(props.markdown, markdown, 'appearance never changes the signed source');
      const style = normalize(props.markdownStyle);
      assert.equal(style.paragraph.color, color, 'ordinary message ink still follows the bubble');
      return style;
    },
  };
}

function over(color: string, backdrop: string): string {
  const alpha = color.length === 9 ? parseInt(color.slice(7), 16) / 255 : 1;
  return mixColor(backdrop, color, alpha);
}

test('real Markdown normalization keeps table cells readable across theme and bubble ink changes', async () => {
  const f = fixture();
  try {
    const markdown = '| Campo | Valor |\n| --- | ---: |\n| alfa | 17 |\n| beta | 23 |';
    for (const profile of ['night', ...THEME_PROFILES, 'day', 'night'] as const) {
      for (const outgoing of [null, '#000000', '#FFFFFF']) {
        const theme = resolveSkyTheme(profile, new Date(2026, 9, 26, 12), {
          ...DEFAULT_THEME_CUSTOMIZATION,
          outgoing,
        });
        for (const ink of [theme.colors.text, theme.colors.onMine]) {
          const style = await f.render(theme, ink, markdown);
          const table = style.table;
          for (const backdrop of [
            theme.presentation.chat.incoming.fill,
            ...theme.presentation.chat.outgoing.fillColors,
          ]) {
            for (const [foreground, background] of [
              [table.headerTextColor!, table.headerBackgroundColor!],
              ...[table.rowEvenBackgroundColor!, table.rowOddBackgroundColor!].flatMap(
                (background) =>
                  [
                    table.color!,
                    style.strong.color ?? table.color!,
                    style.em.color ?? table.color!,
                  ].map((foreground) => [foreground, background]),
              ),
            ])
              assert.ok(
                contrastRatio(foreground!, over(background!, backdrop)) >= 4.5,
                `${profile}: table ink ${foreground} must remain readable on ${background} over ${backdrop}`,
              );
          }
        }
      }
    }
  } finally {
    await f.close();
  }
});

test('quote, inline code and highlighted code retain readable ink on their own surfaces', async () => {
  const f = fixture();
  try {
    const markdown = '> **Quote**\n\n`inline()`\n\n```ts\nconst value = 17; // comment\n```';
    for (const profile of [...THEME_PROFILES, 'night', 'day'] as const) {
      for (const outgoing of ['#000000', '#FFFFFF', null]) {
        const theme = resolveSkyTheme(profile, new Date(2026, 9, 26, 12), {
          ...DEFAULT_THEME_CUSTOMIZATION,
          outgoing,
        });
        for (const ink of [theme.colors.onMine, theme.colors.text]) {
          const style = await f.render(theme, ink, markdown);
          const surfaces = [
            ['quote', style.blockquote.color!, style.blockquote.backgroundColor!],
            [
              'quote bold',
              style.strong.color ?? style.blockquote.color!,
              style.blockquote.backgroundColor!,
            ],
            [
              'quote emphasis',
              style.em.color ?? style.blockquote.color!,
              style.blockquote.backgroundColor!,
            ],
            ['inline code', style.code.color!, style.code.backgroundColor!],
            ['code block', style.codeBlock.color, style.codeBlock.backgroundColor],
            ...Object.entries(style.codeBlock.syntaxColors).map(([token, foreground]) => [
              token,
              foreground,
              style.codeBlock.backgroundColor,
            ]),
          ];
          for (const backdrop of [
            theme.presentation.chat.incoming.fill,
            ...theme.presentation.chat.outgoing.fillColors,
          ])
            for (const [name, foreground, background] of surfaces)
              assert.ok(
                contrastRatio(foreground!, over(background!, backdrop)) >= 4.5,
                `${profile}/${name}: ink ${foreground} must remain readable on ${background} over ${backdrop}`,
              );
        }
      }
    }
  } finally {
    await f.close();
  }
});
