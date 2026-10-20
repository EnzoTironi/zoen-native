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
import * as cardLayout from './app-card-layout.ts';

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
function render(activity: MiniappTimelineActivity) {
  const routes: unknown[] = [];
  const jsx = (type: string, props: Record<string, unknown>) => ({ type, props });
  const modules: Record<string, unknown> = {
    'expo-router': { router: { push: (route: unknown) => routes.push(route) } },
    'react-native': { View: 'View', Text: 'Text', useWindowDimensions: () => ({ width: 402 }) },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react/compiler-runtime': {
      c: (n: number) => Array(n).fill(Symbol.for('react.memo_cache_sentinel')),
    },
    '../core/presentation': {
      messageText: (entry: TimelineEntry) =>
        entry.deleted ? 'This message was deleted' : 'Updated the drawing',
    },
    '../i18n/index.ts': { t, formatLocale: 'en-US' },
    '../theme': {
      useTheme: () => ({ colors: { text: '#fff', secondary: '#ccc', accent: '#9c5' } }),
    },
    '../components/message-interaction': { MessagePressable: 'Pressable' },
    '../components/message-receipt': { MessageReceipt: 'MessageReceipt' },
    './compact-card': { CompactMiniappCard: 'CompactMiniappCard' },
    './app-card-layout': cardLayout,
  };
  const module = { exports: {} as { MiniappTimelineActivityView(props: unknown): Element } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(Object.hasOwn(modules, name), `Unexpected native port: ${name}`);
      return modules[name];
    },
  });
  return { nodes: elements(module.exports.MiniappTimelineActivityView({ activity })), routes };
}

test('compiled iOS activity renders one existing native card and opens real version history, without mutation permissions', () => {
  const events = [edit(2), edit(3)];
  const activity = projectConversationTimeline(events, [current], 'marina')[0]!.activity!;
  const { nodes, routes } = render(activity);
  const cards = nodes.filter((node) => node.type === 'CompactMiniappCard');
  assert.equal(cards.length, 1);
  assert.strictEqual(cards[0]!.props.item, current);
  assert.equal(cards[0]!.props.layout, undefined);
  assert.equal(cards[0]!.props.size, undefined);
  assert.equal(
    cards[0]!.props.onPress,
    undefined,
    'Use the existing permission-gated miniapp route',
  );
  assert.ok(
    nodes.some(
      (node) =>
        node.type === 'Text' &&
        node.props.children === t('miniapps.timelineUpdates', { name: 'Alice', count: 2 }),
    ),
  );
  const history = nodes.find((node) => node.props.testID === 'miniapp-activity-history-event-3')!;
  assert.equal(history.props.accessibilityRole, 'button');
  (history.props.onPress as () => void)();
  assert.equal(
    JSON.stringify(routes),
    JSON.stringify([{ pathname: '/item/[id]', params: { id: 'drawing', history: '1' } }]),
  );
  assert.equal(current.app!.trust, 'Listen');
  assert.deepEqual(activity.entries, events);
});

test('removed item preserves signed note and real history action without fabricating an app preview', () => {
  const activity = projectConversationTimeline([edit(2)], [], 'marina')[0]!.activity!;
  const { nodes } = render(activity);
  assert.ok(!nodes.some((node) => node.type === 'CompactMiniappCard'));
  assert.ok(nodes.some((node) => node.type === 'Text' && node.props.children === 'Adventure'));
  assert.ok(nodes.some((node) => node.props.testID === 'miniapp-activity-history-event-2'));
});

test('deleted activity exposes neither retained title, card nor history action; failed delivery remains explicit', () => {
  const hidden = render({
    entries: [edit(2, { deleted: true })],
    item: current,
    showCard: true,
  }).nodes;
  assert.ok(
    !hidden.some((node) => node.type === 'CompactMiniappCard' || node.type === 'Pressable'),
  );
  assert.ok(!hidden.some((node) => node.type === 'Text' && node.props.children === 'Adventure'));
  const failed = render(
    projectConversationTimeline([edit(2, { delivery: 'Failed' })], [current], 'marina')[0]!
      .activity!,
  ).nodes;
  assert.ok(
    failed.some((node) => node.type === 'MessageReceipt' && node.props.delivery === 'Failed'),
  );
});
