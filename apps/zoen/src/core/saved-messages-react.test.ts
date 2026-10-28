import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { build } from 'esbuild';
import { ClientStore } from './store.ts';
import { Driver, deferred, draftSession, tick } from './store.fixtures.ts';
import * as account from './account-scope.ts';
import { SavedMessagesSession } from '../saved-messages/session.ts';
import type { SpaceSummary } from './generated/SpaceSummary';

const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../saved-messages/use-saved-messages.ts', import.meta.url),
      'utf8',
    ),
    loader: 'ts',
  },
  bundle: true,
  external: ['*'],
  platform: 'node',
  format: 'cjs',
  write: false,
});

async function fixture() {
  let snapshot = draftSession();
  const driver = new Driver();
  driver.opening = async () => snapshot;
  const requests: Array<ReturnType<typeof deferred<SpaceSummary>>> = [];
  driver.operation = async (command) => {
    if (command.op === 'snapshot') return snapshot;
    assert.equal(command.op, 'saved_messages');
    assert.equal(command.expected_identity_id, snapshot.account!.identity_id);
    assert.equal(command.expected_device_id, snapshot.account!.device_id);
    const request = deferred<SpaceSummary>();
    requests.push(request);
    return request.promise;
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const saved: SpaceSummary = {
    ...snapshot.conversations[0]!,
    id: 'canonical-saved',
    kind: 'saved',
  };
  const memo: unknown[] = [];
  let cursor = 0;
  let focus: (() => void | (() => void)) | undefined;
  let cleanup: (() => void) | undefined;
  const opened: unknown[] = [];
  const native = {
    currentState: 'active',
    listeners: new Set<(state: string) => void>(),
    addEventListener(_name: string, changed: (state: string) => void) {
      native.listeners.add(changed);
      return { remove: () => native.listeners.delete(changed) };
    },
  };
  const module = {
    exports: {} as { useSavedMessages(): { retry(): Promise<void>; phase: string } },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name === 'react')
        return {
          useMemo: (create: () => unknown) => {
            const slot = cursor++;
            return (memo[slot] ??= create());
          },
          useCallback: (callback: unknown) => callback,
          useSyncExternalStore: (_subscribe: unknown, read: () => unknown) => read(),
        };
      if (name === 'react-native') return { AppState: native };
      if (name === 'expo-router')
        return {
          router: { replace: (route: unknown) => opened.push(route) },
          useFocusEffect: (effect: typeof focus) => {
            focus = effect;
          },
        };
      if (name === '../core/account-scope') return account;
      if (name === '../core/provider')
        return { useClient: () => ({ store, ...store.getSnapshot() }) };
      if (name === './session') return { SavedMessagesSession };
      throw new Error(`Unexpected port ${name}`);
    },
  });
  const render = () => {
    cursor = 0;
    return module.exports.useSavedMessages();
  };
  render();
  return {
    store,
    requests,
    opened,
    saved,
    render,
    focus: () => {
      cleanup = focus?.() || undefined;
    },
    blur: () => {
      cleanup?.();
      cleanup = undefined;
    },
    background: (state: string) => {
      native.currentState = state;
      for (const changed of native.listeners) changed(state);
    },
    ready: () => {
      snapshot = { ...snapshot, conversations: [saved] };
    },
    close: async () => {
      cleanup?.();
      await store.close({ force: true });
    },
  };
}

test('real hook uses canonical store readiness, not an early writable command result', async () => {
  const f = await fixture();
  f.focus();
  assert.equal(f.requests.length, 1);
  f.requests[0]!.resolve(f.saved);
  await tick();
  assert.equal(f.render().phase, 'waiting');
  assert.equal(f.opened.length, 0);
  f.ready();
  await f.store.refresh();
  assert.equal(f.opened.length, 1);
  assert.deepEqual(JSON.parse(JSON.stringify(f.opened[0])), {
    pathname: '/chat/[id]',
    params: { id: 'canonical-saved' },
  });
  await f.close();
});

test('real AppState invalidates rendered retries and late background navigation', async () => {
  const f = await fixture();
  f.focus();
  const oldRetry = f.render().retry;
  f.background('background');
  f.ready();
  f.requests[0]!.resolve(f.saved);
  await tick();
  assert.equal(f.opened.length, 0);
  f.background('active');
  assert.equal(f.opened.length, 1, 'a fresh foreground lease may open cached canonical history');
  await oldRetry();
  assert.equal(f.requests.length, 1);
  await f.close();
});

test('blur and real account replacement prevent old native work from navigating', async () => {
  for (const changeAccount of [false, true]) {
    const f = await fixture();
    f.focus();
    const oldRetry = f.render().retry;
    if (changeAccount) await f.store.open('demo');
    else f.blur();
    f.ready();
    f.requests[0]!.resolve(f.saved);
    await tick();
    await oldRetry();
    assert.equal(f.opened.length, 0);
    assert.equal(f.requests.length, 1);
    await f.close();
  }
});
