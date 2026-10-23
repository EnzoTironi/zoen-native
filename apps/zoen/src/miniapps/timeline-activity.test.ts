import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import {
  projectConversationTimeline,
  type MiniappTimelineActivity,
} from '../core/conversation-timeline.ts';
import type { ItemDetail, TimelineEntry } from '../core/types';
import { t } from '../i18n/index.ts';
import * as serviceGeometry from './telegram-service-message.ts';

const require = createRequire(import.meta.url);
const sourcePath = new URL('./timeline-activity.tsx', import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const compiled = transformSync(readFileSync(sourcePath, 'utf8'), {
  filename: fileURLToPath(sourcePath),
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
const serviceCompiled = transformSync(
  readFileSync(new URL('../components/service-message.tsx', import.meta.url), 'utf8'),
  {
    filename: fileURLToPath(new URL('../components/service-message.tsx', import.meta.url)),
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
  },
).code;
type Element = { type: string; props: Record<string, unknown> };
function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
const edit = (version: number, patch: Partial<TimelineEntry> = {}): TimelineEntry => ({
  id: `event-${version}`,
  seq: String(version),
  at_ms: String(1000 * version),
  author: { id: 'alice', name: 'Alice', is_me: true } as TimelineEntry['author'],
  kind: {
    ItemEdited: {
      item_id: 'drawing',
      title: 'Adventure',
      version,
      note: 'Updated the drawing',
      is_undo: false,
    },
  },
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
});
const current = {
  id: 'drawing',
  space_id: 'marina',
  kind_id: 'app',
  version: 3,
  title: 'Adventure',
  app: { app_id: 'excalidraw', headline: '5 elements', trust: 'Listen' },
} as ItemDetail;
function render(
  activity: MiniappTimelineActivity,
  options: { disabled?: boolean; highlighted?: boolean } = {},
) {
  const routes: unknown[] = [];
  const jsx = (
    type: string | ((props: Record<string, unknown>) => Element),
    props: Record<string, unknown>,
  ): Element => (typeof type === 'function' ? type(props) : { type, props });
  let cursor = 0;
  const hooks: unknown[] = [];
  const modules: Record<string, unknown> = {
    react: {
      useRef: (value: unknown) => {
        const index = cursor++;
        return (hooks[index] ??= { current: value });
      },
      useState: (value: unknown) => {
        const index = cursor++;
        if (!(index in hooks)) hooks[index] = value;
        return [
          hooks[index],
          (next: unknown) => {
            hooks[index] = typeof next === 'function' ? next(hooks[index]) : next;
          },
        ];
      },
      useCallback: (value: unknown) => value,
      useLayoutEffect: (effect: () => unknown) => effect(),
    },
    'expo-router': { router: { push: (route: unknown) => routes.push(route) } },
    'expo-image': { Image: 'Image' },
    'react-native': {
      View: 'View',
      Text: 'Text',
      useWindowDimensions: () => ({ width: 402, fontScale: 1 }),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'react/compiler-runtime': {
      c: (n: number) => Array(n).fill(Symbol.for('react.memo_cache_sentinel')),
    },
    '../core/presentation': {
      messageText: (entry: TimelineEntry) =>
        entry.deleted
          ? 'This message was deleted'
          : 'ItemEdited' in entry.kind
            ? entry.kind.ItemEdited.note
            : '',
    },
    '../i18n/index.ts': { t },
    '../theme': {
      useTheme: () => ({
        colors: { text: '#fff', secondary: '#ccc', accent: '#9c5' },
        presentation: { chat: { service: { background: '#FFFFFFCC', text: '#6D6D72' } } },
      }),
    },
    '../components/message-interaction': { MessagePressable: 'Pressable' },
    '../components/message-receipt': {
      messageDeliveryLabel: (value: string) => `Delivery: ${value}`,
    },
    './telegram-service-message': serviceGeometry,
  };
  modules['./message-interaction'] = modules['../components/message-interaction'];
  modules['../miniapps/telegram-service-message'] = serviceGeometry;
  const serviceModule = { exports: {} };
  runInNewContext(serviceCompiled, {
    module: serviceModule,
    exports: serviceModule.exports,
    require: (name: string) => modules[name],
  });
  modules['../components/service-message'] = serviceModule.exports;
  const module = { exports: {} as { MiniappTimelineActivityView(props: unknown): Element } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(Object.hasOwn(modules, name), `Unexpected native port: ${name}`);
      return modules[name];
    },
  });
  const draw = (next = activity, props = options) => {
    cursor = 0;
    return elements(module.exports.MiniappTimelineActivityView({ activity: next, ...props }));
  };
  return { nodes: draw(), routes, draw };
}

test('compiled iOS App updates are centered service events with signed attribution and a real history action', () => {
  for (const isMe of [false, true]) {
    const events = [
      edit(2, { author: { ...edit(2).author, is_me: isMe } }),
      edit(3, { author: { ...edit(3).author, is_me: isMe } }),
    ];
    const activity = projectConversationTimeline(events, [current], 'marina')[0]!.activity!;
    const { nodes, routes } = render(activity);
    assert.equal(
      nodes[0]!.props.style && (nodes[0]!.props.style as { alignItems: string }).alignItems,
      'center',
    );
    assert.ok(
      nodes.every(
        (node) => !['CompactMiniappCard', 'MessageReceipt', 'ProfileAvatar'].includes(node.type),
      ),
    );
    const history = nodes.find((node) => node.props.testID === 'miniapp-activity-history-event-3')!;
    assert.equal(history.props.accessibilityRole, 'button');
    assert.equal(
      history.props.accessibilityLabel,
      `${t('miniapps.timelineUpdates', { name: 'Alice', count: 2 })} · Updated the drawing · Adventure`,
    );
    assert.equal(
      history.props.accessibilityHint,
      t('miniapps.timelineHistory', { title: 'Adventure' }),
    );
    (history.props.onPress as () => void)();
    assert.equal(
      JSON.stringify(routes),
      JSON.stringify([{ pathname: '/item/[id]', params: { id: 'drawing', history: '1' } }]),
    );
    assert.equal(current.app!.trust, 'Listen');
    assert.deepEqual(activity.entries, events);
  }
});

test('unavailable, removed, foreign or disabled App updates retain signed event text but no history action', () => {
  for (const item of [undefined, { ...current, space_id: 'other' }, { ...current, version: 0 }]) {
    const activity = projectConversationTimeline([edit(2)], item ? [item] : [], 'marina')[0]!
      .activity!;
    const { nodes, routes } = render(activity);
    assert.ok(nodes.every((node) => node.type !== 'Pressable'));
    assert.ok(
      nodes.some(
        (node) => node.props.accessibilityLabel === 'Alice · Updated the drawing · Adventure',
      ),
    );
    assert.deepEqual(routes, []);
  }
  const activity = projectConversationTimeline([edit(2)], [current], 'marina')[0]!.activity!;
  const disabled = render(activity, { disabled: true });
  assert.ok(disabled.nodes.every((node) => node.type !== 'Pressable'));
  assert.deepEqual(disabled.routes, []);
});

test('deleted activity exposes no retained title, author, note, card or history; pending and failed events stay explicit without receipt ticks', () => {
  const hidden = render({ entries: [edit(2, { deleted: true })], item: current }).nodes;
  assert.ok(
    hidden.every(
      (node) => !['CompactMiniappCard', 'Pressable', 'MessageReceipt'].includes(node.type),
    ),
  );
  assert.ok(hidden.some((node) => node.props.accessibilityLabel === 'This message was deleted'));
  assert.ok(
    hidden.every((node) => !String(node.props.accessibilityLabel).match(/Adventure|Alice|Updated/)),
  );
  for (const delivery of ['Local', 'Sending', 'Failed'] as const) {
    const activity = projectConversationTimeline([edit(2, { delivery })], [current], 'marina')[0]!
      .activity!;
    const nodes = render(activity).nodes;
    assert.ok(
      nodes.some((node) => String(node.props.accessibilityLabel).includes(`Delivery: ${delivery}`)),
    );
    assert.ok(nodes.every((node) => node.type !== 'MessageReceipt'));
  }
});

test('native text layout creates the pinned multiline mask and recycled-row callbacks cannot apply stale geometry', () => {
  const activity = projectConversationTimeline([edit(2)], [current], 'marina')[0]!.activity!;
  const f = render(activity);
  const text = f.nodes.find((node) => node.props.testID === 'miniapp-activity-text-event-2')!;
  const layout = text.props.onTextLayout as (event: unknown) => void;
  layout({
    nativeEvent: {
      lines: [
        { x: 0, y: 0, width: 200, height: 16 },
        { x: 50, y: 16, width: 100, height: 16 },
      ],
    },
  });
  const mask = f
    .draw()
    .find((node) => node.props.testID === 'miniapp-activity-background-event-2')!;
  assert.equal(mask.type, 'Image');
  const source = mask.props.source as { uri: string };
  const svg = decodeURIComponent(source.uri.split(',')[1]!);
  assert.ok(svg.includes('mask-type:luminance'));
  assert.ok(svg.includes('fill="#FFFFFF" fill-opacity="0.8"'));
  const nextActivity = projectConversationTimeline([edit(3)], [current], 'marina')[0]!.activity!;
  f.draw(nextActivity);
  layout({ nativeEvent: { lines: [{ x: 0, y: 0, width: 999, height: 99 }] } });
  const recycled = f.draw(nextActivity);
  assert.ok(
    recycled.every((node) => node.type !== 'Image'),
    'Old content never draws behind a new signed event',
  );
});
