import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { documentScope } from './document-authority.ts';
import { homeSpacesFor } from '../home/access.ts';

const compile = async (path: string) =>
  (
    await build({
      stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
      bundle: true,
      platform: 'node',
      format: 'cjs',
      write: false,
      external: ['*'],
    })
  ).outputFiles[0]!.text;
const queryCode = await compile('../core/query.ts');
const hookCode = await compile('./page-image-items.ts');

function fixture() {
  const state: any = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: { identity_id: 'alice', device_id: 'phone', unlocked: true, setup_pending: false },
      me: { id: 'alice' },
      conversations: [{ id: 'chat', members: [{ id: 'alice' }] }],
    },
  };
  const canonical = new Set<(spaces?: string[]) => void>(),
    preflight = new Set<() => void>();
  const background = new Set<() => void>();
  const calls: { command: unknown; resolve: (items: unknown) => void }[] = [];
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    driver: {
      request: (command: unknown) => new Promise((resolve) => calls.push({ command, resolve })),
    },
    subscribeCanonicalChanges: (fn: (spaces?: string[]) => void) => {
      canonical.add(fn);
      return () => canonical.delete(fn);
    },
    registerSessionPreflight: (fn: () => void) => {
      preflight.add(fn);
      return () => preflight.delete(fn);
    },
  };
  let focused = true;
  const app = {
    currentState: 'active',
    addEventListener: (_: string, fn: () => void) => {
      background.add(fn);
      return { remove: () => background.delete(fn) };
    },
  };
  const slots: any[] = [];
  let cursor = 0;
  const queued: (() => void)[] = [];
  const memo = (fn: () => unknown, deps: unknown[]) => {
    const index = cursor++,
      old = slots[index];
    if (
      !old ||
      deps.length !== old.deps.length ||
      !deps.every((value, n) => Object.is(value, old.deps[n]))
    )
      slots[index] = { value: fn(), deps };
    return slots[index].value;
  };
  const effect = (fn: () => any, deps: unknown[]) => {
    const index = cursor++,
      old = slots[index];
    if (
      !old ||
      deps.length !== old.deps.length ||
      !deps.every((value, n) => Object.is(value, old.deps[n]))
    ) {
      slots[index] = { deps };
      queued.push(() => {
        old?.cleanup?.();
        slots[index].cleanup = fn();
      });
    }
  };
  const react = {
    useState: (initial: unknown) => {
      const index = cursor++;
      if (!(index in slots)) slots[index] = initial;
      return [
        slots[index],
        (next: any) => {
          slots[index] = typeof next === 'function' ? next(slots[index]) : next;
        },
      ];
    },
    useRef: (initial: unknown) => memo(() => ({ current: initial }), []),
    useMemo: memo,
    useCallback: (fn: () => unknown, deps: unknown[]) => memo(() => fn, deps),
    useEffect: effect,
    useLayoutEffect: effect,
    useSyncExternalStore: (subscribe: (listener: () => void) => () => void, get: () => unknown) => {
      effect(() => subscribe(() => {}), [subscribe]);
      return get();
    },
  };
  const router = {
    useIsFocused: () => focused,
    useFocusEffect: (fn: () => any) => effect(() => (focused ? fn() : undefined), [fn, focused]),
  };
  let query: any;
  const load = (code: string) => {
    const module = { exports: {} as any };
    runInNewContext(code, {
      module,
      require(name: string) {
        if (name === 'react') return react;
        if (name === 'react-native') return { AppState: app };
        if (name === 'expo-router') return router;
        if (['./provider', '../core/provider'].includes(name))
          return { useClient: () => ({ ...state, store }) };
        if (name === '../core/query') return query;
        if (name === '../home/access') return { homeSpacesFor };
        if (name === './document-authority') return { documentScope };
        throw new Error(`Unexpected module ${name}`);
      },
    });
    return module.exports;
  };
  query = load(queryCode);
  const hook = load(hookCode);
  let page: any = {
    item_id: 'page',
    space_id: 'chat',
    path: 'page.md',
    ready: true,
    version: 2,
    content_hash: 'page-hash',
  };
  const render = () => {
    cursor = 0;
    return hook.usePageImageItems(page, true);
  };
  const commit = () => {
    for (let n = 0; n < 4; n++) {
      render();
      while (queued.length) queued.shift()!();
    }
    return render();
  };
  return {
    state,
    store,
    calls,
    render,
    commit,
    canonical: (spaces?: string[]) => {
      for (const fn of canonical) fn(spaces);
    },
    preflight: () => {
      for (const fn of preflight) fn();
    },
    focus: (next: boolean) => {
      focused = next;
    },
    background: (next: string) => {
      app.currentState = next;
      for (const fn of background) fn();
    },
    page: (patch: any) => {
      page = { ...page, ...patch };
    },
  };
}
const settle = async () => {
  for (let n = 0; n < 12; n++) await Promise.resolve();
};

async function loaded() {
  const f = fixture();
  f.commit();
  const items = [{ id: 'photo', version: 1 }];
  f.calls.at(-1)!.resolve(items);
  await settle();
  const view = f.commit();
  assert.equal(view.current(), true);
  return { f, view, items };
}

test('actual hook/query reads one canonical index and invalidates the current lease synchronously before refresh', async () => {
  const { f, view } = await loaded();
  const count = f.calls.length;
  assert.deepEqual(JSON.parse(JSON.stringify(f.calls.at(-1)!.command)), { op: 'items' });
  f.canonical(['other-chat']);
  assert.equal(view.current(), true);
  f.commit();
  assert.equal(f.calls.length, count);
  f.canonical(['chat']);
  assert.equal(view.current(), false);
  assert.equal(
    f.render().items,
    null,
    'cached data cannot render under the new canonical revision',
  );
  f.commit();
  assert.equal(f.calls.length, count + 1);
  f.calls.at(-1)!.resolve([{ id: 'photo', version: 2 }]);
  await settle();
  const fresh = f.commit();
  assert.equal(fresh.current(), true);
  assert.equal(view.current(), false);
});

test('late old query results cannot populate a newer Page/history generation', async () => {
  const f = fixture();
  f.commit();
  const old = f.calls.at(-1)!;
  f.page({ version: 1, content_hash: 'historical-page' });
  assert.equal(f.render().items, null);
  f.commit();
  const next = f.calls.at(-1)!;
  old.resolve([{ id: 'old-result' }]);
  await settle();
  assert.equal(f.commit().items, null);
  next.resolve([{ id: 'historical-file' }]);
  await settle();
  assert.equal(f.commit().items[0].id, 'historical-file');
});

test('blur/background/owner/device/epoch/membership/preflight permanently retire old handlers', async () => {
  for (const cut of [
    'blur',
    'background',
    'owner',
    'device',
    'epoch',
    'membership',
    'locked',
    'erase',
    'preflight',
  ]) {
    const { f, view } = await loaded();
    if (cut === 'blur') {
      f.focus(false);
      f.commit();
    }
    if (cut === 'background') f.background('background');
    if (cut === 'owner') f.state.snapshot.account.identity_id = 'bob';
    if (cut === 'device') f.state.snapshot.account.device_id = 'replacement';
    if (cut === 'epoch') f.store.sessionEpoch++;
    if (cut === 'membership') f.state.snapshot.conversations[0].members = [];
    if (cut === 'locked') f.state.snapshot.account.unlocked = false;
    if (cut === 'erase') f.state.erasePending = true;
    if (cut === 'preflight') f.preflight();
    assert.equal(view.current(), false, cut);
    assert.equal(f.commit().items, null, cut);
  }
});

test('same-owner refocus and fast background/foreground cannot resurrect an old image callback', async () => {
  const { f, view, items } = await loaded();
  f.background('background');
  f.background('active');
  assert.equal(view.current(), false);
  f.commit();
  f.calls.at(-1)!.resolve(items);
  await settle();
  assert.equal(f.commit().current(), true);
  assert.equal(view.current(), false, 'reused array/owner gets a new lease');
  const prior = f.commit();
  f.focus(false);
  f.commit();
  f.focus(true);
  f.commit();
  f.calls.at(-1)!.resolve(items);
  await settle();
  assert.equal(f.commit().current(), true);
  assert.equal(prior.current(), false);
});
