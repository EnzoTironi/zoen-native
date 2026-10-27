import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { build } from 'esbuild';
import { HomeController, projectHomeItems, type HomeOwner } from '../home/preferences.ts';
import type { ItemDetail } from './generated/ItemDetail';
import { t } from '../i18n/index.ts';

type Element = { type: string | ((props: any) => Element); props: Record<string, any> };
const external = new Set([
  'react',
  'react/jsx-runtime',
  'react-native',
  'react-native-reanimated',
  'expo-router',
  '@shopify/flash-list',
  '../../../core/provider',
  '../../../theme',
  '../../../i18n/index.ts',
  '../i18n',
  '../../../home/provider',
  '../../../components/primitives',
  '../components/primitives',
  '../../../components/chats-actions',
  '../../../profile/profile-avatar',
  '../../../components/message-receipt',
  '../../../miniapps/rail',
  '../../../inbox/chat-row',
  '../../../inbox/use-inbox-actions',
  '../../../inbox/action-feedback',
]);
const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../app/(tabs)/(chats)/chats.tsx', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'header-edit-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          external.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  if (typeof node.type === 'function') return nodes(node.type(node.props));
  return [node, ...nodes(node.props.children), ...nodes(node.props.ListHeaderComponent)];
}
async function fixture() {
  const owner: HomeOwner = { mode: 'account', identityId: 'alice', deviceId: 'phone' };
  const item = (id: string) =>
    ({ id, space_id: 'chat', app: { app_id: 'deckcraft' } }) as ItemDetail;
  const writes: string[] = [];
  const controller = new HomeController(
    {
      owner: () => owner,
      spaces: () => new Set(['chat']),
      items: async () => [item('first'), item('second')],
    },
    {
      get: async () => null,
      set: async (_key, value) => {
        writes.push(value);
      },
    },
  );
  await controller.activate();
  const home = () => {
    const state = controller.getSnapshot();
    return {
      ...state,
      ...projectHomeItems(state.items, state),
      ownerKey: 'alice:phone',
      beginEdit: () => controller.beginEdit(owner),
      discardEdit: () => controller.discardEdit(owner),
      commitEdit: () => controller.commitEdit(owner),
      requireEdit: () => controller.requireEdit(owner),
      reorderItems: (ids: string[], expected: string[]) =>
        controller.reorderItems(ids, owner, expected),
      hide: (id: string) => controller.hide(id, owner),
      pin: (id: string) => controller.pin(id, owner),
      unpin: (id: string) => controller.unpin(id, owner),
      isPinned: (id: string) => state.pinnedIds.includes(id),
    };
  };
  const jsx = (type: Element['type'], props: Element['props']) => ({ type, props });
  const primitives = { IconButton: 'IconButton', Icon: 'Icon', EmptyState: 'EmptyState' };
  const ports: Record<string, unknown> = {
    react: {
      useState: (value: unknown) => [value, () => {}],
      useMemo: (read: () => unknown) => read(),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': {
      View: 'View',
      Text: 'Text',
      Pressable: 'Pressable',
      ScrollView: 'ScrollView',
      StyleSheet: { hairlineWidth: 0.5 },
    },
    'expo-router': { Stack: { Screen: 'Screen' }, router: { push() {} } },
    '@shopify/flash-list': { FlashList: 'FlashList' },
    '../../../core/provider': { useClient: () => ({ snapshot: { conversations: [] }, store: {} }) },
    '../../../theme': {
      useTheme: () => ({
        presentation: {
          text: '#123456',
          typography: { largeTitle: {} },
          chatList: { message: '#555555' },
        },
      }),
    },
    '../../../home/provider': { useHome: home },
    '../../../i18n/index.ts': { t },
    '../i18n': { t },
    '../../../components/primitives': primitives,
    '../components/primitives': primitives,
    '../../../components/chats-actions': { ChatsActions: 'ChatsActions' },
    '../../../profile/profile-avatar': { ProfileAvatar: 'ProfileAvatar' },
    '../../../components/message-receipt': { MessageReceipt: 'MessageReceipt' },
    '../../../miniapps/rail': { MiniappRail: 'MiniappRail' },
    // Inbox lifecycle/gestures are exercised by the real compiled inbox suites.
    '../../../inbox/chat-row': { InboxChatRow: 'InboxChatRow' },
    '../../../inbox/action-feedback': { InboxActionFeedback: () => null },
    '../../../inbox/use-inbox-actions': {
      useInboxActions: () => ({ enabled: false, state: {}, session: {}, ticket: null }),
    },
  };
  const module = { exports: {} as { default(): Element } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const render = () => nodes(module.exports.default());
  return { render, controller, writes };
}

test('actual Chats header puts Discard/Done between title and search/compose and uses the same controller draft', async () => {
  const f = await fixture();
  const normal = f.render();
  assert.ok(normal.every((n) => n.props.testID !== 'app-card-edit-controls'));
  const rail = normal.find((n) => n.type === 'MiniappRail')!;
  assert.equal(rail.props.paddingVertical, 2);
  rail.props.onEditingChange(true);
  let editing = f.render();
  const title = editing.findIndex((n) => n.props.testID === 'chats-title');
  const controls = editing.findIndex((n) => n.props.testID === 'app-card-edit-controls');
  const actions = editing.findIndex((n) => n.type === 'ChatsActions');
  assert.ok(title < controls && controls < actions);
  assert.ok(editing.every((n) => n.props.testID !== 'app-rail-rearrange-done'));
  const liveRail = editing.find((n) => n.type === 'MiniappRail')!;
  await liveRail.props.onRemove({ id: 'first' });
  assert.deepEqual(f.controller.getSnapshot().hiddenIds, ['first']);
  assert.deepEqual(f.writes, []);
  editing.find((n) => n.props.testID === 'app-card-edit-discard')!.props.onPress();
  assert.equal(f.controller.getSnapshot().editing, false);
  assert.deepEqual(f.controller.getSnapshot().hiddenIds, []);
  assert.deepEqual(f.writes, []);
  assert.throws(
    () => liveRail.props.onRemove({ id: 'second' }),
    'Retired header callbacks cannot become immediate writes',
  );
  f.render()
    .find((n) => n.type === 'MiniappRail')!
    .props.onEditingChange(true);
  editing = f.render();
  await editing.find((n) => n.type === 'MiniappRail')!.props.onRemove({ id: 'first' });
  editing.find((n) => n.props.testID === 'app-card-edit-done')!.props.onPress();
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(f.controller.getSnapshot().editing, false);
  assert.equal(f.writes.length, 1);
  assert.deepEqual(JSON.parse(f.writes[0]!).hiddenIds, ['first']);
});
