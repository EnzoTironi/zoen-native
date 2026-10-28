import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import type { ConversationMessageProps } from '../components/conversation-message';
import { resolveSkyTheme, type SkyPhase } from '../theme/sky.ts';
import { transformMessageModules } from './message-source-compiler.ts';
import type { TimelineEntry } from './types';

export const require = createRequire(import.meta.url);
export const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
export const sourcePath = fileURLToPath(
  new URL('../components/conversation-message.tsx', import.meta.url),
);
export const source = readFileSync(process.env.ZOEN_MESSAGE_CAPTION_SOURCE ?? sourcePath, 'utf8');
export const compiled = transformSync(source, {
  filename: sourcePath,
  configFile: false,
  babelrc: false,
  presets: [require.resolve('babel-preset-expo')],
  caller: {
    name: 'metro',
    platform: 'ios',
    engine: 'hermes',
    isDev: false,
    supportsReactCompiler: true,
  },
}).code;
export const nativePorts = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  'react-native-gesture-handler',
  'react-native-reanimated',
  'react-native-worklets',
  'expo-haptics',
  'expo-router',
  '../i18n/index.ts',
  '../media/MediaAttachment',
  '../miniapps/message-card',
  '../profile/profile-avatar',
  '../theme',
  './primitives',
  './message-receipt',
  './service-message',
  './message-markdown',
  './use-semantic-message-preview',
]);
export const output = await build({
  stdin: {
    contents: `${compiled}\nObject.assign(exports, require('./message-interaction'));`,
    resolveDir: dirname(sourcePath),
    loader: 'js',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'message-caption-native-ports',
      setup(builder) {
        transformMessageModules(builder, 'ios');
        builder.onResolve({ filter: /./ }, ({ path }) =>
          nativePorts.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

export type Element = { type: unknown; props: Record<string, unknown> };
export function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
export function textValues(value: unknown): unknown[] {
  return elements(value)
    .filter((node) => node.type === 'Text')
    .map((node) => node.props.children);
}
export function styleOf(node: Element): Record<string, unknown> {
  const style = node.props.style;
  return (typeof style === 'function' ? style({ pressed: false }) : style) as Record<
    string,
    unknown
  >;
}
export function entry(
  text = 'A real signed caption',
  patch: Partial<TimelineEntry> = {},
): TimelineEntry {
  return {
    id: 'caption_message',
    seq: '1',
    author: {
      id: 'sender',
      name: 'Sender',
      kind: 'Person',
      is_me: false,
    } as TimelineEntry['author'],
    at_ms: '1000',
    kind: {
      Message: {
        text,
        card: { item_id: 'signed_file', kind_id: 'file', title: 'video.mp4', version: 1 },
      },
    } as TimelineEntry['kind'],
    delivery: 'Sent',
    reply_to: null,
    in_thread: null,
    thread_replies: 0,
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
    voice: null,
    ...patch,
  };
}

export function render(
  message: TimelineEntry,
  {
    phase = 'day',
    disabled = false,
    next,
    previous,
    albumPart,
    revealAnchor,
    previewText,
  }: {
    phase?: SkyPhase;
    previewText?: string | null;
    disabled?: boolean;
    next?: TimelineEntry;
    previous?: TimelineEntry;
    albumPart?: ConversationMessageProps['albumPart'];
    revealAnchor?: { id: string; onLayout: Function; onTextLayout: Function };
  } = {},
) {
  const theme = resolveSkyTheme(phase, new Date('2026-10-18T12:00:00'));
  const calls: { mode: string; entry: TimelineEntry }[] = [];
  const routes: unknown[] = [];
  const quotes: string[] = [];
  const previews: unknown[] = [];
  let didPan = false;
  let compilerCursor = 0;
  const caches: unknown[][] = [];
  let hookCursor = 0;
  const hooks: unknown[] = [];
  let viewportWidth = 390;
  let viewportFontScale = 1;
  const makeElement = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const modules: Record<string, unknown> = {
    react: {
      memo: (component: unknown) => component,
      useRef: (value: unknown) => {
        const slot = hookCursor++;
        return (hooks[slot] ??= { current: value });
      },
      useState: (value: unknown) => {
        const slot = hookCursor++;
        if (!(slot in hooks)) hooks[slot] = value;
        return [
          hooks[slot],
          (next: unknown) => {
            hooks[slot] = typeof next === 'function' ? next(hooks[slot]) : next;
          },
        ];
      },
      useCallback: (value: unknown) => value,
      useLayoutEffect: (effect: () => unknown) => effect(),
      createContext: (value: unknown) => ({ value }),
      use: (context: { value: unknown }) =>
        context.value === undefined
          ? { get: () => didPan }
          : context.value === null
            ? (revealAnchor ?? null)
            : context.value,
    },
    'react/compiler-runtime': {
      c: (size: number) =>
        (caches[compilerCursor++] ??= Array(size).fill(Symbol.for('react.memo_cache_sentinel'))),
    },
    'react/jsx-runtime': { jsx: makeElement, jsxs: makeElement },
    'react-native': {
      Text: 'Text',
      View: 'View',
      Pressable: 'NativePressable',
      useWindowDimensions: () => ({
        width: viewportWidth,
        height: 844,
        scale: 3,
        fontScale: viewportFontScale,
      }),
    },
    'react-native-gesture-handler': {},
    'react-native-reanimated': { default: {} },
    'react-native-worklets': {},
    'expo-haptics': {},
    'expo-router': { router: { push: (value: unknown) => routes.push(value) } },
    '../i18n/index.ts': { t: (key: string) => key, locale: 'en', formatLocale: 'en-US' },
    '../media/MediaAttachment': { MessageMedia: 'MessageMedia' },
    '../miniapps/message-card': { MiniappMessageCard: 'MiniappMessageCard' },
    '../profile/profile-avatar': { ProfileAvatar: 'ProfileAvatar' },
    '../theme': { useTheme: () => theme },
    './primitives': { Icon: 'Icon' },
    './service-message': { ServiceMessage: 'ServiceMessage' },
    './message-markdown': { MessageMarkdown: 'MessageMarkdown' },
    './use-semantic-message-preview': {
      useSemanticMessagePreview: (input: { text: string }) => {
        previews.push(input);
        return { text: previewText === null ? undefined : (previewText ?? input.text) };
      },
    },
    './message-receipt': { MessageReceipt: 'MessageReceipt', messageDeliveryLabel: () => 'Sent' },
  };
  const module = {
    exports: {} as {
      ConversationMessage(props: Record<string, unknown>): Element;
      MessageInteraction: unknown;
      MessagePressable(props: Record<string, unknown>): Element;
    },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(
        Object.hasOwn(modules, name),
        `Unexpected native import in caption renderer: ${name}`,
      );
      return modules[name];
    },
  });
  const draw = (value = message, width = viewportWidth, fontScale = viewportFontScale) => {
    compilerCursor = 0;
    hookCursor = 0;
    viewportWidth = width;
    viewportFontScale = fontScale;
    return module.exports.ConversationMessage({
      spaceId: 'synthetic_chat',
      entry: value,
      previous,
      next,
      groupChat: true,
      disabled,
      albumPart,
      onReply(value: TimelineEntry) {
        calls.push({ mode: 'reply', entry: value });
      },
      onThread(value: TimelineEntry) {
        calls.push({ mode: 'thread', entry: value });
      },
      onAuthor() {},
      onReact() {},
      onQuote(id: string) {
        quotes.push(id);
      },
      onMenu: (value: TimelineEntry, mode: string) => {
        calls.push({ mode, entry: value });
      },
    });
  };
  const tree = draw();
  const caption = elements(tree).find(
    (node) => node.props.testID === `message-caption-${message.id}`,
  );
  return {
    tree,
    routes,
    quotes,
    previews,
    rerender: draw,
    caption,
    theme,
    calls,
    pan: (value: boolean) => {
      didPan = value;
    },
    interactionType: module.exports.MessageInteraction,
    pressableType: module.exports.MessagePressable,
    nativeCaption: () => {
      assert.ok(caption);
      assert.equal(caption.type, module.exports.MessagePressable);
      return module.exports.MessagePressable(caption.props);
    },
  };
}

export function plainMessage(text = 'OK', patch: Partial<TimelineEntry> = {}): TimelineEntry {
  const base = entry(text);
  return {
    ...base,
    kind: { Message: { text, card: null } } as TimelineEntry['kind'],
    author: { ...base.author, is_me: true },
    ...patch,
  };
}
export function nodeById(tree: unknown, id: string): Element {
  const node = elements(tree).find((node) => node.props.testID === id);
  assert.ok(node, id);
  return node;
}
export function measureText(
  tree: unknown,
  id: string,
  {
    right = 20,
    count = 1,
    width = 20,
    statusWidth = 60,
    height = 21,
  }: { right?: number; count?: number; width?: number; statusWidth?: number; height?: number } = {},
) {
  const body = nodeById(tree, `message-body-${id}`);
  const status = nodeById(tree, `message-metadata-${id}`);
  (body.props.onTextLayout as Function)({
    nativeEvent: { lines: Array.from({ length: count }, () => ({ x: 0, width: right, height })) },
  });
  (body.props.onLayout as Function)({ nativeEvent: { layout: { width } } });
  (status.props.onLayout as Function)({
    nativeEvent: { layout: { width: statusWidth, height: 14 } },
  });
}
