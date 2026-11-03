import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { build } from 'esbuild';
import { ClientStore } from './store.ts';
import { deferred, Driver, draftSession, tick } from './store.fixtures.ts';
import * as links from './message-link.ts';
import { hookHost } from './message-selection-react.fixtures.ts';
import type { TimelineEntry } from './types';

const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../app/message/[space]/[message].tsx', import.meta.url),
      'utf8',
    ),
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
function find(value: any, type: string): Element | undefined {
  if (Array.isArray(value)) return value.map((child) => find(child, type)).find(Boolean);
  if (!value || typeof value !== 'object') return;
  return value.type === type ? value : find(value.props?.children, type);
}
async function fixture() {
  const host = hookHost(),
    driver = new Driver(),
    snapshot = draftSession();
  driver.opening = async () => snapshot;
  let read = async () => ({ id: 'message', deleted: false, in_thread: 'root' }) as TimelineEntry;
  driver.operation = async (command) => {
    assert.equal(command.op, 'message');
    return read();
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const routes: unknown[] = [];
  const module = { exports: {} as { default(): Element } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react') return host.react;
      if (name === 'react/jsx-runtime')
        return {
          jsx: (type: string, props: unknown) => ({ type, props }),
          jsxs: (type: string, props: unknown) => ({ type, props }),
          Fragment: 'Fragment',
        };
      if (name === 'expo-router')
        return {
          router: { replace: (route: unknown) => routes.push(JSON.parse(JSON.stringify(route))) },
          Stack: { Screen: 'Screen' },
          useFocusEffect: host.useFocusEffect,
          useLocalSearchParams: () => ({
            space: snapshot.conversations[0]!.id,
            message: 'message',
          }),
        };
      if (name.endsWith('/core/provider')) return { useClient: () => ({ store }) };
      if (name.endsWith('/core/message-link')) return links;
      if (name.endsWith('/components/primitives'))
        return { Loading: 'Loading', EmptyState: 'EmptyState', NativeButton: 'Button' };
      if (name.endsWith('/i18n/index.ts')) return { t: (key: string) => key };
      throw new Error(name);
    },
  });
  return {
    routes,
    store,
    host,
    render: () => host.render(() => module.exports.default()),
    read: (next: typeof read) => {
      read = next;
    },
  };
}

test('actual link route replaces itself with the canonical thread and preserves exact target', async () => {
  const f = await fixture();
  try {
    assert.ok(find(f.render(), 'Loading'));
    await tick();
    assert.deepEqual(f.routes, [
      {
        pathname: '/thread/[space]/[root]',
        params: { space: 'read-chat', root: 'root', jump: 'message' },
      },
    ]);
  } finally {
    f.host.blur();
    await f.store.close();
  }
});
test('actual link route shows an unavailable result and explicit retry recovers without reopening history', async () => {
  const f = await fixture();
  try {
    f.read(async () => {
      throw new Error('not local');
    });
    f.render();
    await tick();
    const empty = find(f.render(), 'EmptyState');
    assert.ok(empty);
    assert.equal(empty.props.title, 'chat.messageUnavailable');
    assert.deepEqual(f.routes, []);
    f.read(async () => ({ id: 'message', deleted: false, in_thread: null }) as TimelineEntry);
    find(empty, 'Button')!.props.onPress();
    f.render();
    await tick();
    assert.deepEqual(f.routes, [
      { pathname: '/chat/[id]', params: { id: 'read-chat', jump: 'message' } },
    ]);
  } finally {
    f.host.blur();
    await f.store.close();
  }
});
test('a blurred link route cannot navigate when its old canonical read completes', async () => {
  const f = await fixture();
  try {
    const pending = deferred<TimelineEntry>();
    f.read(() => pending.promise);
    f.render();
    f.host.blur();
    pending.resolve({ id: 'message', deleted: false, in_thread: 'root' } as TimelineEntry);
    await tick();
    assert.deepEqual(f.routes, []);
  } finally {
    await f.store.close();
  }
});
