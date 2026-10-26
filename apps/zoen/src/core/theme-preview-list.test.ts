import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { THEME_PROFILES } from '../theme/catalog.ts';
import { DEFAULT_THEME_CUSTOMIZATION, telegramPresentation } from '../theme/telegram.ts';

type Element = { type: unknown; props: Record<string, any> };
const element = (type: unknown, props: Record<string, any>): Element => ({ type, props });
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
async function load(file: string, modules: Record<string, unknown>) {
  const output = await build({
    stdin: {
      contents: readFileSync(new URL(`../components/${file}.tsx`, import.meta.url), 'utf8'),
      loader: 'tsx',
    },
    platform: 'node',
    format: 'cjs',
    bundle: true,
    jsx: 'automatic',
    write: false,
    plugins: [
      {
        name: 'ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  });
  const module = { exports: {} as Record<string, (props?: any) => Element> };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      assert.ok(Object.hasOwn(modules, name), name);
      return modules[name];
    },
  });
  return module.exports;
}
const jsx = { jsx: element, jsxs: element };
test('chat preview uses current presentation for bubbles, text, wallpaper and receipt', async () => {
  const presentation = telegramPresentation('night', {
    ...DEFAULT_THEME_CUSTOMIZATION,
    textSize: 23,
  });
  const sky = { sentinel: true };
  const modules = await load('theme-chat-preview', {
    'react/jsx-runtime': jsx,
    'react-native': { View: 'View', Text: 'Text' },
    '../i18n': { t: (key: string) => key },
    '../theme': { useTheme: () => ({ presentation, sky }) },
    './message-graphics': { MessageBubbleGraphic: 'Bubble', MessageReceiptGraphic: 'Receipt' },
    './sky-background': { SkyLayer: 'Sky' },
  });
  const tree = nodes(modules.ThemeChatPreview!());
  assert.equal(tree.find((n) => n.type === 'Sky')!.props.sky, sky);
  const bubbles = tree.filter((n) => n.type === 'Bubble');
  assert.equal(bubbles[0]!.props.fill, presentation.chat.incoming.fill);
  assert.equal(bubbles[1]!.props.fillColors, presentation.chat.outgoing.fillColors);
  assert.equal(
    tree.find((n) => n.props.children === 'settings.themePreviewOwn')!.props.style.fontSize,
    23,
  );
  assert.equal(
    tree.find((n) => n.type === 'Receipt')!.props.color,
    presentation.chat.outgoing.secondary,
  );
});
test('preset list reveals selection after measured layout and on selection change', async () => {
  const calls: any[] = [];
  const refs: any[] = [];
  let cursor = 0;
  let effect: (() => void) | undefined;
  const modules = await load('theme-preset-list', {
    'react/jsx-runtime': jsx,
    react: {
      useCallback: (fn: unknown) => fn,
      useRef: (value: unknown) => refs[cursor++] ?? (refs[cursor - 1] = { current: value }),
      useLayoutEffect: (fn: () => void) => {
        effect = fn;
      },
    },
    'react-native': { ScrollView: 'ScrollView' },
    '../theme': { useTheme: () => ({ reducedMotion: false }) },
    '../theme/catalog': { THEME_PROFILES },
  });
  function draw(profile: string) {
    cursor = 0;
    const tree = modules.ThemePresetList!({ profile, children: null });
    refs[0].current = { scrollTo: (value: unknown) => calls.push(value) };
    effect!();
    return tree;
  }
  const first = draw(THEME_PROFILES[0]!);
  first.props.onLayout({ nativeEvent: { layout: { width: 232 } } });
  assert.equal(calls.length, 0);
  first.props.onContentSizeChange(708);
  assert.equal(calls[0].x, 0);
  assert.equal(calls[0].animated, false);
  draw(THEME_PROFILES[5]!);
  assert.equal(calls[1].x, 476);
  assert.equal(calls[1].animated, true);
});
