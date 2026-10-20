import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { homeOwnerFor, homeSpacesFor } from '../home/access.ts';
import * as homePreferences from '../home/preferences.ts';
import * as conversationPreferences from '../miniapps/conversation-preferences.ts';
import type { ItemDetail } from './generated/ItemDetail';
import type { Snapshot } from './types';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const sourcePath = new URL('../miniapps/conversation-provider.tsx', import.meta.url);
// Optional negative-control input lets the identical assertions exercise an older hook.
const source = readFileSync(process.env.ZOEN_CONVERSATION_PROJECTION_SOURCE ?? sourcePath, 'utf8');
const compiled = transformSync(source, {
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

const miniapp = (id: string): ItemDetail =>
  ({ id, title: id, space_id: 'shared', app: { app_id: 'poll' } }) as ItemDetail;
const ids = (items: readonly ItemDetail[]) => items.map((item) => item.id);
type Projection = { items: readonly ItemDetail[]; visible: readonly ItemDetail[] };

async function fixture() {
  let client: Parameters<typeof homeOwnerFor>[0] & { typing?: number } = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      me: { id: 'alice' },
      account: { identity_id: 'alice', device_id: 'phone', unlocked: true, setup_pending: false },
      conversations: [{ id: 'shared', members: [{ id: 'alice' }] }],
    } as unknown as Snapshot,
  };
  let canonical: homePreferences.HomeState = {
    owner: homeOwnerFor(client),
    items: [miniapp('first'), miniapp('second'), miniapp('third')],
    pinnedIds: [],
    orderedIds: [],
    loading: false,
    error: null,
  };
  const port = { get: async () => null, set: async () => {} };
  const controller = new conversationPreferences.ConversationCardsController(
    {
      owner: () => homeOwnerFor(client),
      spaces: () => homeSpacesFor(client),
      items: () => canonical.items,
    },
    port,
  );
  await controller.activate();
  const home = { getSnapshot: () => canonical, subscribe: () => () => {}, refresh: async () => {} };
  // Preserve the compiler's cache between renders, exactly where the stale v42
  // projection lived. The controller and owner references intentionally stay fixed.
  const cache: unknown[] = [];
  let compiledHookCalls = 0;
  const modules: Record<string, unknown> = {
    react: {
      createContext: () => ({}),
      use: () => controller,
      useMemo: (calculate: () => unknown) => calculate(),
      useSyncExternalStore: (_subscribe: unknown, getSnapshot: () => unknown) => getSnapshot(),
    },
    'react/compiler-runtime': {
      c: (size: number) => {
        ++compiledHookCalls;
        if (!cache.length) cache.push(...Array(size).fill(Symbol.for('react.memo_cache_sentinel')));
        return cache;
      },
    },
    'react/jsx-runtime': {},
    '../core/provider': { useClient: () => client },
    '../home/provider': { useHomeController: () => home, homeOwnerFor },
    '../home/access': { homeOwnerFor, homeSpacesFor },
    '../home/preferences': homePreferences,
    '../i18n': { t: (key: string) => key },
    '../preferences': { preferences: port },
    './conversation-preferences': conversationPreferences,
  };
  const exports = {} as { useConversationCards: (spaceId: string) => Projection };
  runInNewContext(compiled, {
    exports,
    module: { exports },
    require: (name: string) => {
      assert.ok(Object.hasOwn(modules, name), `Unexpected native import in hook test: ${name}`);
      return modules[name];
    },
  });
  return {
    controller,
    render: () => {
      const result = exports.useConversationCards('shared');
      assert.ok(compiledHookCalls > 0, 'Exercise compiled output, not an uncompiled hook bailout');
      return result;
    },
    updateCanonical: async (items: ItemDetail[]) => {
      canonical = { ...canonical, items };
      await controller.refresh();
    },
    type: () => {
      client = { ...client, typing: (client.typing ?? 0) + 1 };
    },
  };
}

test('compiled conversation hook refreshes same-owner hide, show and reorder without reopening', async () => {
  const f = await fixture();
  assert.deepEqual(ids(f.render().visible), ['first', 'second', 'third']);
  await f.controller.setPinned('shared', 'first', false);
  assert.deepEqual(ids(f.render().visible), ['second', 'third']);
  await f.controller.setPinned('shared', 'first', true);
  assert.deepEqual(ids(f.render().visible), ['first', 'second', 'third']);
  await f.controller.reorder('shared', ['third', 'first', 'second']);
  assert.deepEqual(ids(f.render().visible), ['third', 'first', 'second']);
});

test('compiled conversation hook observes canonical changes while typing keeps projected arrays stable', async () => {
  const f = await fixture();
  const before = f.render();
  f.type();
  assert.equal(f.render().visible, before.visible);
  assert.equal(f.render().items, before.items);
  await f.updateCanonical([miniapp('first'), miniapp('second'), miniapp('third'), miniapp('new')]);
  assert.deepEqual(ids(f.render().visible), ['first', 'second', 'third', 'new']);
});
