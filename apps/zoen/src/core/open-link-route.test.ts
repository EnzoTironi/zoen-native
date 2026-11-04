import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { hookHost } from './message-selection-react.fixtures.ts';
import { deferred, tick } from './store.fixtures.ts';

const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('../app/open-link.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  jsx: 'automatic',
  external: ['*'],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
});

type Element = { type: string; props: Record<string, any> };

function button(node: Element | Element[]): Element | undefined {
  if (Array.isArray(node)) return node.map(button).find(Boolean);
  if (!node?.props) return;
  return node.type === 'Button' ? node : button(node.props.children);
}

function fixture() {
  const host = hookHost();
  const routes: unknown[] = [];
  let status = 'pending';
  const listeners = new Set<() => void>();
  const joins: string[] = [];
  const pending = deferred<[string, string]>();
  const snapshot = { account: { identity_id: 'person', device_id: 'device' } };
  const store = {
    sessionEpoch: 1,
    getSnapshot: () => ({ snapshot }),
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    driver: {
      request: async (command: { space_id: string; client_id: string }) => {
        assert.equal(command.space_id, 'joined-space');
        assert.equal(command.client_id, 'join-receipt');
        return status;
      },
    },
    request: ({ code }: { code: string }) => {
      joins.push(code);
      return pending.promise;
    },
  };
  let target = 'first';
  const module = { exports: {} as { default(): Element } };
  const jsx = (type: string, props: unknown) => ({ type, props });
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require(name: string) {
      switch (name) {
        case 'react':
          return host.react;
        case 'react/jsx-runtime':
          return { jsx, jsxs: jsx };
        case 'expo-router':
          return {
            router: { replace: (route: unknown) => routes.push(route) },
            Stack: { Screen: 'Screen' },
            useFocusEffect: host.useFocusEffect,
            useLocalSearchParams: () => ({ kind: 'invite', target }),
          };
        case 'react-native':
          return { View: 'View', Text: 'Text' };
        case '../core/provider':
          return { useClient: () => ({ store, snapshot }) };
        case '../core/query':
          return { useCoreQuery: () => ({ data: { title: 'Group', members: 2 } }) };
        case '../notifications/links':
          return {
            parseZoenLink: (url: string) => ({ kind: 'invite', code: url.split('/').at(-1) }),
          };
        case '../theme':
          return { useTheme: () => ({ colors: {} }) };
        case '../components/primitives':
          return { EmptyState: 'EmptyState', NativeButton: 'Button' };
        case '../i18n/index.ts':
          return { t: (key: string) => key };
        default:
          throw new Error(name);
      }
    },
  });
  return {
    host,
    routes,
    joins,
    pending,
    store,
    listeners,
    render: () => button(host.render(() => module.exports.default()))!.props,
    setTarget: (value: string) => {
      target = value;
    },
    update: (value: string) => {
      status = value;
      for (const listener of listeners) listener();
    },
  };
}

test('invite admission rejects an old route callback and late navigation after session replacement', async () => {
  const f = fixture();
  const old = f.render().onPress;
  f.setTarget('second');
  const current = f.render().onPress;
  old();
  assert.deepEqual(f.joins, []);
  current();
  current();
  assert.deepEqual(f.joins, ['second']);
  f.store.sessionEpoch++;
  f.pending.resolve(['joined-space', 'join-receipt']);
  await tick();
  assert.deepEqual(f.routes, []);
  f.host.blur();
});

test('queued group invite opens only after the exact membership appears', async () => {
  const f = fixture();
  f.render().onPress();
  f.pending.resolve(['joined-space', 'join-receipt']);
  await tick();
  const waiting = f.render();
  assert.equal(waiting.label, 'chat.waitingForInviteConfirmation');
  assert.equal(waiting.disabled, true);
  waiting.onPress();
  assert.deepEqual(f.joins, ['first']);
  assert.equal(f.routes.length, 0);
  f.update('joined');
  await tick();
  assert.equal(f.routes.length, 1);
  assert.equal(f.listeners.size, 0);
  f.host.blur();
});

test('refusal of the exact invite leaves the screen open and clears its pending action', async () => {
  const f = fixture();
  f.render().onPress();
  f.pending.resolve(['joined-space', 'join-receipt']);
  await tick();
  f.update('refused');
  await tick();
  assert.equal(f.routes.length, 0);
  assert.equal(f.listeners.size, 0);
  assert.equal(f.render().disabled, false);
  f.host.blur();
});
