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
import type { ConversationMessageProps } from '../components/conversation-message';

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
    previous,
    albumPart,
  }: {
    phase?: SkyPhase;
    disabled?: boolean;
    next?: TimelineEntry;
    previous?: TimelineEntry;
    albumPart?: ConversationMessageProps['albumPart'];
  } = {},
) {
  const theme = resolveSkyTheme(phase, new Date('2026-10-18T12:00:00'));
  const calls: { mode: string; entry: TimelineEntry }[] = [];
  const routes: unknown[] = [];
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
        context.value === undefined ? { get: () => didPan } : context.value,
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
      onQuote() {},
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

test('file captions retain the selected Telegram palette, text size and independent caption layout', () => {
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
      if (phase === 'day' || !mine) assert.ok(contrastRatio(ink as string, paper as string) >= 4.5);
      else
        assert.deepEqual(
          f.theme.presentation.chat.outgoing.fillColors,
          ['#61BCF9', '#0088FF'],
          'retain the pinned default gradient; custom colors have their own contrast test',
        );
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

test('decoded image dimensions fit the actual bubble, caption and metadata and recompute on narrow and rotated viewports', () => {
  for (const mine of [false, true]) {
    const message = entry();
    message.author = { ...message.author, is_me: mine };
    const f = render(message);
    const media = elements(f.tree).find((node) => node.type === 'MessageMedia')!;
    const decode = media.props.onImageDecoded as (image: Record<string, unknown>) => void;
    decode({ itemId: 'signed_file', version: 1, sha256: 'a'.repeat(64), width: 700, height: 1000 });
    let tree = f.rerender();
    const bubble = (value: Element) =>
      elements(value).find((node) => node.props.testID === 'message-bubble-caption_message')!;
    assert.ok(Math.abs(Number(styleOf(bubble(tree)).width) - 252) < 1e-6);
    const caption = elements(bubble(tree)).find(
      (node) => node.props.testID === 'message-caption-caption_message',
    )!;
    assert.ok(caption, 'Caption stays within the fitted image bubble');
    assert.equal(
      styleOf(caption).width,
      undefined,
      'Caption stretches to its bubble rather than a separate fixed width',
    );
    if (!mine)
      assert.ok(
        elements(tree).some(
          (node) =>
            Math.abs(Number(styleOf(node)?.width) - 252) < 1e-6 &&
            styleOf(node)?.maxWidth === '100%',
        ),
        'Incoming metadata shares the same fitted width',
      );
    tree = f.rerender(message, 240);
    assert.ok(Number(styleOf(bubble(tree)).width) <= 240 - 32 - (mine ? 0 : 44));
    decode({
      itemId: 'signed_file',
      version: 1,
      sha256: 'a'.repeat(64),
      width: 2000,
      height: 1000,
    });
    const narrowWidth = Number(styleOf(bubble(f.rerender())).width);
    const wideWidth = Number(styleOf(bubble(f.rerender(message, 900))).width);
    assert.ok(
      wideWidth > narrowWidth,
      'Fit uses current space rather than retaining the prior fitted width',
    );
    assert.equal(wideWidth, 294);
  }
});

test('late decoded dimensions from a replaced file version cannot overwrite the new image geometry', () => {
  const original = entry();
  const f = render(original);
  const oldDecode = elements(f.tree).find((node) => node.type === 'MessageMedia')!.props
    .onImageDecoded as (image: Record<string, unknown>) => void;
  const replacement = entry();
  assert.ok('Message' in replacement.kind && replacement.kind.Message.card);
  replacement.kind.Message.card.version = 2;
  let tree = f.rerender(replacement);
  const decode = elements(tree).find((node) => node.type === 'MessageMedia')!.props
    .onImageDecoded as (image: Record<string, unknown>) => void;
  decode({ itemId: 'signed_file', version: 2, sha256: 'b'.repeat(64), width: 700, height: 1000 });
  tree = f.rerender(replacement);
  const width = () =>
    styleOf(elements(tree).find((node) => node.props.testID === 'message-bubble-caption_message')!)
      .width;
  assert.ok(Math.abs(Number(width()) - 252) < 1e-6);
  oldDecode({
    itemId: 'signed_file',
    version: 1,
    sha256: 'a'.repeat(64),
    width: 2000,
    height: 1000,
  });
  tree = f.rerender(replacement);
  assert.ok(Math.abs(Number(width()) - 252) < 1e-6);
  decode({ itemId: 'another-file', version: 2, sha256: 'c'.repeat(64), width: 2000, height: 1000 });
  tree = f.rerender(replacement);
  assert.ok(Math.abs(Number(width()) - 252) < 1e-6);
});

function plainMessage(text = 'OK', patch: Partial<TimelineEntry> = {}): TimelineEntry {
  const base = entry(text);
  return {
    ...base,
    kind: { Message: { text, card: null } } as TimelineEntry['kind'],
    author: { ...base.author, is_me: true },
    ...patch,
  };
}
function nodeById(tree: unknown, id: string): Element {
  const node = elements(tree).find((node) => node.props.testID === id);
  assert.ok(node, id);
  return node;
}
function measureText(
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

test('plain text uses compact Telegram metrics while canonical App/media presentation keeps its policy', () => {
  const msg = plainMessage();
  const f = render(msg);
  const bubble = nodeById(f.tree, `message-bubble-${msg.id}`);
  const style = styleOf(bubble);
  assert.equal(style.maxWidth, 345);
  assert.equal(style.minWidth, 40);
  assert.equal(style.minHeight, 35);
  assert.equal(style.borderRadius, 16);
  assert.equal(style.paddingHorizontal, 11);
  assert.equal(style.paddingTop, 6 + 1 / 3);
  assert.equal(style.paddingBottom, 6 - 1 / 3);
  assert.equal(styleOf(nodeById(f.tree, `message-row-${msg.id}`)).marginTop, 2 + 1 / 3);
  assert.equal(styleOf(nodeById(f.tree, `message-body-${msg.id}`)).fontSize, 17);
  assert.equal(styleOf(nodeById(f.tree, `message-body-${msg.id}`)).lineHeight, undefined);
  const app = entry('App activity', {
    author: { ...entry().author, kind: 'Agent' },
    kind: {
      Message: {
        text: 'App activity',
        card: { item_id: 'app', kind_id: 'app', app: { app_id: 'deckcraft' } },
      },
    } as TimelineEntry['kind'],
  });
  const appTree = render(app).tree;
  assert.equal(styleOf(nodeById(appTree, `message-bubble-${app.id}`)).width, 164);
  const appMetadata = nodeById(appTree, `message-metadata-${app.id}`);
  assert.equal(styleOf(appMetadata).paddingBottom, 6);
  assert.equal(styleOf(appMetadata).paddingHorizontal, 11);
  assert.equal(
    elements(appTree).find((node) => node.type === 'MiniappMessageCard')?.props.width,
    160,
  );
  assert.equal(nodeById(appTree, `message-body-${app.id}`).props.onTextLayout, undefined);
});

test('core grouping drives zero merged spacing and the correct 8pt same-side corners', () => {
  for (const mine of [false, true]) {
    const msg = plainMessage('One', { author: { ...entry().author, is_me: mine } });
    const previous = { ...msg, id: 'previous' };
    const next = { ...msg, id: 'next' };
    const f = render(msg, { previous, next });
    const style = styleOf(nodeById(f.tree, `message-bubble-${msg.id}`));
    assert.equal(styleOf(nodeById(f.tree, `message-row-${msg.id}`)).marginTop, 0);
    assert.equal(style[mine ? 'borderTopRightRadius' : 'borderTopLeftRadius'], 8);
    assert.equal(style[mine ? 'borderBottomRightRadius' : 'borderBottomLeftRadius'], 8);
    assert.equal(style[mine ? 'borderTopLeftRadius' : 'borderTopRightRadius'], 16);
  }
});

test('actual native trailing-line measurements place time inline without estimating characters', () => {
  const msg = plainMessage('OK');
  const f = render(msg);
  assert.equal(styleOf(nodeById(f.tree, `message-metadata-${msg.id}`)).position, undefined);
  measureText(f.tree, msg.id);
  const tree = f.rerender();
  assert.equal(styleOf(nodeById(tree, `message-bubble-${msg.id}`)).minWidth, 113);
  assert.equal(styleOf(nodeById(tree, `message-metadata-${msg.id}`)).position, 'absolute');
  assert.equal(styleOf(nodeById(tree, `message-metadata-${msg.id}`)).right, 17);
  assert.equal(textValues(tree).filter((text) => text === 'OK').length, 1);
  const receipt = elements(tree).find((node) => node.type === 'MessageReceipt');
  assert.equal(receipt?.props.delivery, msg.delivery);
});

test('wrapped text only shares a trailing line that has measured space; RTL and tall metadata keep a row', () => {
  for (const [text, right, height, inline] of [
    ['A wrapped message', 160, 21, true],
    ['A wrapped message', 280, 21, false],
    ['مرحبا hello', 20, 21, false],
    ['A wrapped message', 160, 10, false],
  ] as const) {
    const msg = plainMessage(text);
    const f = render(msg);
    measureText(f.tree, msg.id, { right, count: 2, width: 300, height });
    assert.equal(
      styleOf(nodeById(f.rerender(), `message-metadata-${msg.id}`)).position,
      inline ? 'absolute' : undefined,
    );
  }
});

test('recycled message, viewport, delivery and font changes reject stale native layout callbacks', () => {
  const original = plainMessage('Original');
  const f = render(original);
  const oldBody = nodeById(f.tree, `message-body-${original.id}`);
  const oldStatus = nodeById(f.tree, `message-metadata-${original.id}`);
  const next = plainMessage('New owner', { id: 'next_owner', delivery: 'Read', edited: true });
  let tree = f.rerender(next);
  (oldBody.props.onTextLayout as Function)({
    nativeEvent: { lines: [{ x: 0, width: 20, height: 21 }] },
  });
  (oldBody.props.onLayout as Function)({ nativeEvent: { layout: { width: 300 } } });
  (oldStatus.props.onLayout as Function)({ nativeEvent: { layout: { width: 60, height: 14 } } });
  tree = f.rerender(next);
  assert.equal(styleOf(nodeById(tree, `message-metadata-${next.id}`)).position, undefined);
  measureText(tree, next.id);
  tree = f.rerender(next);
  assert.equal(styleOf(nodeById(tree, `message-metadata-${next.id}`)).position, 'absolute');
  tree = f.rerender(next, 240, 2);
  assert.equal(styleOf(nodeById(tree, `message-metadata-${next.id}`)).position, undefined);
  assert.equal(styleOf(nodeById(tree, `message-bubble-${next.id}`)).maxWidth, 195);
  assert.equal(
    elements(tree).find((node) => node.type === 'MessageReceipt')?.props.delivery,
    'Read',
  );
});

test('plain bubble ink and paper retain every actual Zoen sky theme and message side', () => {
  for (const phase of SKY_PHASES)
    for (const mine of [false, true]) {
      const msg = plainMessage('Read me', { author: { ...entry().author, is_me: mine } });
      const f = render(msg, { phase });
      const graphic = elements(f.tree).find(
        (node) => typeof node.type === 'function' && node.type.name === 'MessageBubbleGraphic',
      );
      assert.ok(graphic);
      const paper = graphic.props.fill as string;
      const ink = styleOf(nodeById(f.tree, `message-body-${msg.id}`)).color as string;
      if (phase === 'day' || !mine) assert.ok(contrastRatio(ink, paper) >= 4.5);
      else assert.equal(ink, '#FFFFFF', 'the default Night gradient uses the pinned white ink');
      assert.equal(paper, mine ? f.theme.colors.mine : f.theme.colors.theirs);
    }
});

test('plain bubble fill uses viewport coordinates before edge and author insets at the exact 500pt boundary', () => {
  const cases = [
    [240, 195, 151],
    [402, 357, 313],
    [500, 455, 411],
    [501, 416, 372],
    [520, 433, 389],
    [120, 75, 40],
  ] as const;
  for (const [width, ownBound, incomingBound] of cases)
    for (const mine of [true, false]) {
      const msg = plainMessage('A long enough actual text body', {
        author: { ...entry().author, is_me: mine },
      });
      const f = render(msg);
      const tree = f.rerender(msg, width);
      const bound = styleOf(nodeById(tree, `message-bubble-${msg.id}`)).maxWidth;
      assert.equal(bound, mine ? ownBound : incomingBound, `${width}pt mine=${mine}`);
      assert.ok(
        Number(bound) <= width - 32 - (mine ? 0 : 44),
        'Never overflow the actual available row slot',
      );
    }
});

test('text and App shares own the native background; file and voice boundaries retain their own rendering', () => {
  for (const mine of [true, false]) {
    const msg = plainMessage('Editable text', { author: { ...entry().author, is_me: mine } });
    const f = render(msg);
    const bubble = nodeById(f.tree, `message-bubble-${msg.id}`);
    const graphics = elements(f.tree).filter(
      (node) => typeof node.type === 'function' && node.type.name === 'MessageBubbleGraphic',
    );
    assert.equal(graphics.length, 1);
    assert.equal(graphics[0]!.props.ownerKey, msg.id);
    assert.equal(graphics[0]!.props.incoming, !mine);
    assert.equal(
      styleOf(bubble).backgroundColor,
      'transparent',
      'Sky remains visible through the actual curved tail cutout',
    );
    assert.equal(styleOf(bubble).paddingLeft, mine ? 11 : 17);
    assert.equal(styleOf(bubble).paddingRight, mine ? 17 : 11);
  }
  for (const msg of [
    entry('caption'),
    entry('', { voice: { item_id: 'voice' } as TimelineEntry['voice'] }),
    entry('App', {
      kind: {
        Message: {
          text: 'App',
          card: { item_id: 'app', kind_id: 'app', app: { app_id: 'deckcraft' } },
        },
      } as TimelineEntry['kind'],
    }),
  ]) {
    assert.equal(
      elements(render(msg).tree).filter(
        (node) => typeof node.type === 'function' && node.type.name === 'MessageBubbleGraphic',
      ).length,
      'Message' in msg.kind &&
        msg.kind.Message.card?.kind_id === 'app' &&
        msg.author.kind === 'Agent'
        ? 1
        : 0,
    );
  }
});

test('actual renderer binds receipt contrast to bubble gradient stops or the separate media metadata surface', () => {
  for (const phase of SKY_PHASES) {
    for (const file of [false, true]) {
      const message = entry('Exact canonical content', { delivery: 'Read' });
      message.author = { ...message.author, is_me: true };
      if (!file)
        message.kind = {
          Message: { text: 'Exact canonical content', card: null },
        } as TimelineEntry['kind'];
      const f = render(message, { phase });
      const receipt = elements(f.tree).find((node) => node.type === 'MessageReceipt');
      assert.ok(receipt);
      assert.equal(receipt.props.delivery, 'Read');
      if (file) {
        assert.equal(
          receipt.props.backgrounds,
          undefined,
          'media metadata is outside its caption bubble',
        );
        assert.equal(receipt.props.background, f.theme.colors.background);
        assert.equal(receipt.props.ink, f.theme.colors.secondary);
      } else {
        assert.deepEqual(receipt.props.backgrounds, f.theme.presentation.chat.outgoing.fillColors);
        assert.equal(receipt.props.background, f.theme.presentation.chat.outgoing.fill);
        assert.equal(receipt.props.ink, f.theme.presentation.chat.outgoing.text);
      }
      assert.equal(
        textValues(f.tree).filter((text) => text === 'Exact canonical content').length,
        1,
      );
    }
  }
});

test('album cells keep the actual canonical renderer and per-item reply, thread, menu and decode targets', () => {
  const message = entry('Caption for the second canonical member');
  let decoded: unknown;
  const tile = { width: 120, height: 200 };
  const f = render(message, {
    albumPart: {
      tile,
      onDecoded: (image) => {
        decoded = image;
      },
    },
  });
  const interaction = elements(f.tree).find((node) => node.type === f.interactionType)!;
  assert.ok(interaction);
  assert.equal(interaction.props.messageId, message.id);
  (interaction.props.onReply as () => void)();
  (interaction.props.onThread as () => void)();
  (interaction.props.onActions as () => void)();
  assert.deepEqual(
    f.calls.map((call) => call.mode),
    ['reply', 'thread', 'actions'],
  );
  assert.ok(f.calls.every((call) => call.entry === message));
  const media = elements(f.tree).find((node) => node.type === 'MessageMedia')!;
  assert.strictEqual(media.props.entry, message);
  assert.deepEqual(media.props.tile, tile);
  const dimensions = {
    itemId: 'signed_file',
    version: 1,
    sha256: 'a'.repeat(64),
    width: 40,
    height: 80,
  };
  (media.props.onImageDecoded as (image: unknown) => void)(dimensions);
  assert.strictEqual(decoded, dimensions);
  assert.equal(
    textValues(f.tree).includes(
      message.kind && 'Message' in message.kind ? message.kind.Message.text : '',
    ),
    false,
  );
  const caption = render(message, { albumPart: { captionOnly: true } });
  assert.equal(
    textValues(caption.tree).filter((value) => value === 'Caption for the second canonical member')
      .length,
    1,
  );
  assert.equal(
    elements(caption.tree).some((node) => node.type === 'MessageMedia'),
    false,
  );
});

test('typed human App shares show one160pt cover without caption/bubble; Agent text stays exact', () => {
  const caption = 'Apresentação adicionada a esta conversa';
  const card = {
    item_id: 'deck',
    kind_id: 'app',
    title: 'Presentation',
    version: 1,
    app: { app_id: 'deckcraft' },
  };
  for (const kind of ['Person', 'Agent'] as const) {
    const message = entry(caption, {
      author: { ...entry().author, kind },
      kind: { Message: { text: caption, card } } as TimelineEntry['kind'],
    });
    const f = render(message);
    const all = elements(f.tree);
    const body = all.filter((n) => n.props.testID === `message-body-${message.id}`);
    const cover = all.find((n) => n.type === 'MiniappMessageCard')!;
    assert.ok(cover);
    assert.equal(cover.props.card, card);
    assert.equal(cover.props.width, 160);
    assert.equal(body.length, kind === 'Agent' ? 1 : 0);
    assert.equal(
      all.some((n) => n.props.testID === `message-bubble-${message.id}`),
      kind === 'Agent',
    );
    if (kind === 'Agent') assert.equal(body[0]!.props.children, caption);
    else {
      assert.ok(all.some((n) => n.props.testID === `message-app-card-${message.id}`));
      assert.ok(all.every((n) => n.props.children !== caption));
      assert.ok(all.some((n) => n.props.testID === `message-metadata-${message.id}`));
    }
  }
});

test('approval and resolved receipts retain canonical text in the shared compact face and exact review route', () => {
  for (const approved of [null, true, false]) {
    const f = render(
      entry('', {
        kind: {
          Request: { request_id: 'canonical_request', title: 'Read presentation', approved },
        },
      }),
    );
    assert.equal(f.tree.type, 'ServiceMessage');
    assert.equal(f.tree.props.label, 'Read presentation');
    (f.tree.props.onPress as () => void)();
    assert.deepEqual(JSON.parse(JSON.stringify(f.routes)), [
      { pathname: '/request/[id]', params: { id: 'canonical_request' } },
    ]);
  }
  const receipt = 'Approved once. Waiting for the reviewed action.';
  const f = render(entry('', { kind: { System: { text: receipt } } }));
  assert.equal(f.tree.type, 'ServiceMessage');
  assert.equal(f.tree.props.label, receipt);
  assert.equal(f.tree.props.onPress, undefined);
  const disabled = render(
    entry('', { kind: { Request: { request_id: 'r', title: 'Denied', approved: false } } }),
    { disabled: true },
  );
  assert.equal(disabled.tree.props.onPress, undefined);
});

test('formatted bodies, file captions and album captions share native Markdown without changing signed source', () => {
  const text = '**Title**\n\n- one\n- two\n\n```ts\nconst exact = 1;\n```';
  const message = plainMessage(text);
  const body = render(message);
  const formatted = nodeById(body.tree, `message-body-${message.id}`);
  assert.equal(formatted.type, 'MessageMarkdown');
  assert.equal(formatted.props.text, text);
  assert.equal(formatted.props.maxWidth, 317);
  assert.equal(formatted.props.onTextLayout, undefined);
  assert.equal(styleOf(nodeById(body.tree, `message-metadata-${message.id}`)).position, undefined);
  const file = entry(text);
  for (const options of [{}, { albumPart: { captionOnly: true } }]) {
    const f = render(file, options);
    const native = elements(f.tree).find((node) => node.type === 'MessageMarkdown')!;
    assert.ok(native);
    assert.equal(native.props.text, text);
    assert.equal(native.props.maxWidth, 245.18);
  }
  const app = entry(text, {
    author: { ...file.author, kind: 'Agent' },
    kind: {
      Message: { text, card: { item_id: 'deck', kind_id: 'app', app: { app_id: 'deckcraft' } } },
    } as TimelineEntry['kind'],
  });
  const appBody = nodeById(render(app).tree, `message-body-${app.id}`);
  assert.equal(appBody.type, 'MessageMarkdown');
  assert.equal(appBody.props.text, text);
  assert.equal(appBody.props.maxWidth, 142);
});

test('formatting retires prior inline status measurements; possible embedded media stays exact plain text', () => {
  const original = plainMessage('Short');
  const f = render(original);
  measureText(f.tree, original.id);
  assert.equal(
    styleOf(nodeById(f.rerender(), `message-metadata-${original.id}`)).position,
    'absolute',
  );
  const text = '**Formatted**';
  const next = { ...original, kind: { Message: { text, card: null } } };
  const changed = f.rerender(next);
  assert.equal(nodeById(changed, `message-body-${original.id}`).type, 'MessageMarkdown');
  assert.equal(styleOf(nodeById(changed, `message-metadata-${original.id}`)).position, undefined);
  for (const text of [
    '**Bold** ![image](https://example.com/image)',
    '**Code**\n```c\nx < y\n```',
    '<video src="https://example.com/video"></video>',
  ]) {
    const next = plainMessage(text);
    const rendered = render(next);
    assert.equal(nodeById(rendered.tree, `message-body-${next.id}`).type, 'Text');
    assert.equal(nodeById(rendered.tree, `message-body-${next.id}`).props.children, text);
  }
});
