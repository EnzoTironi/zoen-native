import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { contrastRatio, resolveSkyTheme, SKY_PHASES, type SkyPhase } from '../theme/sky.ts';
import type { TimelineEntry } from './types';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const sourcePath = fileURLToPath(
  new URL('../components/conversation-message.tsx', import.meta.url),
);
const source = readFileSync(process.env.ZOEN_MESSAGE_CAPTION_SOURCE ?? sourcePath, 'utf8');
const compiled = transformSync(source, {
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
const nativePorts = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  'react-native-gesture-handler',
  'react-native-reanimated',
  'react-native-worklets',
  'expo-haptics',
  '../i18n/index.ts',
  '../media/MediaAttachment',
  '../miniapps/message-card',
  '../profile/profile-avatar',
  '../theme',
  './primitives',
  './message-receipt',
]);
const output = await build({
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
        builder.onResolve({ filter: /./ }, ({ path }) =>
          nativePorts.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

type Element = { type: unknown; props: Record<string, unknown> };
function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
function textValues(value: unknown): unknown[] {
  return elements(value)
    .filter((node) => node.type === 'Text')
    .map((node) => node.props.children);
}
function styleOf(node: Element): Record<string, unknown> {
  const style = node.props.style;
  return (typeof style === 'function' ? style({ pressed: false }) : style) as Record<
    string,
    unknown
  >;
}
function entry(text = 'A real signed caption', patch: Partial<TimelineEntry> = {}): TimelineEntry {
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

function render(
  message: TimelineEntry,
  {
    phase = 'day',
    disabled = false,
    next,
  }: { phase?: SkyPhase; disabled?: boolean; next?: TimelineEntry } = {},
) {
  const theme = resolveSkyTheme(phase, new Date('2026-10-18T12:00:00'));
  const calls: { mode: string; entry: TimelineEntry }[] = [];
  let didPan = false;
  let compilerCursor = 0;
  const caches: unknown[][] = [];
  const makeElement = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const modules: Record<string, unknown> = {
    react: {
      memo: (component: unknown) => component,
      useRef: (value: unknown) => ({ current: value }),
      useLayoutEffect: (effect: () => unknown) => effect(),
      createContext: (value: unknown) => ({ value }),
      use: () => ({ get: () => didPan }),
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
      useWindowDimensions: () => ({ width: 390, height: 844 }),
    },
    'react-native-gesture-handler': {},
    'react-native-reanimated': { default: {} },
    'react-native-worklets': {},
    'expo-haptics': {},
    '../i18n/index.ts': { t: (key: string) => key, locale: 'en', formatLocale: 'en-US' },
    '../media/MediaAttachment': { MessageMedia: 'MessageMedia' },
    '../miniapps/message-card': { MiniappMessageCard: 'MiniappMessageCard' },
    '../profile/profile-avatar': { ProfileAvatar: 'ProfileAvatar' },
    '../theme': { useTheme: () => theme },
    './primitives': { Icon: 'Icon' },
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
  const tree = module.exports.ConversationMessage({
    spaceId: 'synthetic_chat',
    entry: message,
    next,
    groupChat: true,
    disabled,
    onReply() {},
    onThread() {},
    onAuthor() {},
    onReact() {},
    onQuote() {},
    onMenu: (value: TimelineEntry, mode: string) => {
      calls.push({ mode, entry: value });
    },
  });
  const caption = elements(tree).find(
    (node) => node.props.testID === `message-caption-${message.id}`,
  );
  return {
    tree,
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

test('actual message renderer shows exact canonical file caption once below the media boundary', () => {
  for (const caption of [
    '  Caption with spaces\nand another line 🇧🇷  ',
    'Arquivo: video.mp4',
    'video.mp4',
  ]) {
    const f = render(entry(caption, { forwarded: true }));
    assert.ok(f.caption);
    assert.equal(textValues(f.tree).filter((text) => text === caption).length, 1);
    const bubble = elements(f.tree).find(
      (node) => node.props.testID === 'message-bubble-caption_message',
    );
    assert.ok(bubble);
    assert.equal(bubble.type, 'View', 'Native media keeps its non-pressable ancestor');
    const contents = elements(bubble.props.children);
    const mediaIndex = contents.findIndex((node) => node.type === 'MessageMedia');
    assert.ok(mediaIndex >= 0);
    assert.ok(contents.indexOf(f.caption) > mediaIndex);
    assert.equal(
      styleOf(f.caption).position,
      undefined,
      'Caption is laid out below arbitrary image pixels',
    );
  }
});

test('file captions use actual readable message paper and ink for each sky and message side', () => {
  for (const phase of SKY_PHASES) {
    for (const mine of [false, true]) {
      const message = entry();
      const f = render({ ...message, author: { ...message.author, is_me: mine } }, { phase });
      assert.ok(f.caption);
      const paper = styleOf(f.caption).backgroundColor;
      const text = elements(f.caption).find((node) => node.type === 'Text');
      assert.ok(text);
      const ink = styleOf(text).color;
      assert.equal(paper, mine ? f.theme.colors.mine : f.theme.colors.theirs);
      assert.equal(ink, mine ? f.theme.colors.onMine : f.theme.colors.text);
      assert.ok(contrastRatio(ink as string, paper as string) >= 4.5, `${phase}, mine=${mine}`);
      assert.equal(styleOf(text).fontSize, 17);
      assert.equal(styleOf(text).lineHeight, 23);
      assert.ok(Number(styleOf(f.caption).minHeight) >= 44);
    }
  }
});

test('deleted, voice, miniapp and blank messages never render a file caption', () => {
  const retained = 'Private retained caption';
  const messages = [
    entry(retained, { deleted: true }),
    entry(retained, { voice: { item_id: 'voice' } as TimelineEntry['voice'] }),
    entry(retained, {
      kind: {
        Message: { text: retained, card: { item_id: 'miniapp', kind_id: 'app' } },
      } as TimelineEntry['kind'],
    }),
    entry(' \n\t '),
    entry('', {}),
  ];
  for (const message of messages) {
    const f = render(message);
    assert.equal(f.caption, undefined);
    if (message.deleted || message.voice) assert.ok(!textValues(f.tree).includes(retained));
  }
  const plain = 'An ordinary text message';
  const f = render(entry(plain, { kind: { Message: { text: plain, card: null } } }));
  assert.equal(f.caption, undefined);
  assert.equal(textValues(f.tree).filter((text) => text === plain).length, 1);
});

test('caption native press and hold use existing actions while consumed swipes suppress trailing presses', () => {
  const message = entry();
  const f = render(message);
  assert.ok(f.caption);
  assert.ok('Message' in message.kind);
  assert.equal(f.caption.props.accessibilityRole, 'button');
  assert.equal(f.caption.props.accessibilityLabel, `Sender: ${message.kind.Message.text}`);
  const native = f.nativeCaption();
  (native.props.onPress as () => void)();
  (native.props.onLongPress as () => void)();
  assert.equal(f.calls[0]?.mode, 'reactions');
  assert.equal(f.calls[1]?.mode, 'actions');
  assert.equal(f.calls[0]?.entry, message);
  f.pan(true);
  (native.props.onPress as () => void)();
  (native.props.onLongPress as () => void)();
  assert.equal(f.calls.length, 2);
  (native.props.onAccessibilityTap as () => void)();
  assert.equal(f.calls.length, 3, 'Accessibility action bypasses physical swipe consumption');
  assert.equal(f.calls[2]?.mode, 'actions');
  const interaction = elements(f.tree).find((node) => node.type === f.interactionType);
  assert.ok(interaction);
  assert.ok(elements(interaction).includes(f.caption));
});

test('disabled captions stay readable without opening message actions', () => {
  const f = render(entry(), { disabled: true });
  assert.ok(f.caption);
  assert.equal(f.caption.props.disabled, true);
  assert.equal(f.caption.props.accessibilityHint, undefined);
  const native = f.nativeCaption();
  (native.props.onPress as () => void)();
  (native.props.onLongPress as () => void)();
  assert.equal(f.calls.length, 0);
});

test('caption tails follow the existing incoming and outgoing run grouping', () => {
  for (const mine of [false, true]) {
    const message = entry();
    message.author = { ...message.author, is_me: mine };
    const ended = render(message);
    const continued = render(message, { next: { ...message, id: 'next', at_ms: '2000' } });
    assert.ok(ended.caption && continued.caption);
    const corner = mine ? 'borderBottomRightRadius' : 'borderBottomLeftRadius';
    assert.equal(styleOf(ended.caption)[corner], 6);
    assert.equal(styleOf(continued.caption)[corner], 22);
  }
});
