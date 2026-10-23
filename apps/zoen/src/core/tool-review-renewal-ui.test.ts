import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as flow from './tool-review-renewal.ts';
import { createLocalization } from '../i18n/runtime.ts';
import { reviewRenewal } from '../i18n/catalogs/review-renewal.ts';

const files = {
  view: '../agents/tool-review-recovery.tsx',
  hook: '../agents/use-tool-review.ts',
  card: '../agents/request-card.tsx',
};
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const compiled = new Map(
  await Promise.all(
    Object.entries(files).map(async ([name, path]) => {
      const source = readFileSync(
        name === 'view' && process.env.ZOEN_RENEWAL_VIEW_SOURCE
          ? process.env.ZOEN_RENEWAL_VIEW_SOURCE
          : new URL(path, import.meta.url),
        'utf8',
      );
      const transformed =
        name === 'view'
          ? transformSync(source, {
              filename: fileURLToPath(new URL(path, import.meta.url)),
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
            }).code
          : source;
      if (name === 'view') assert.match(transformed, /react\/compiler-runtime/);
      const result = await build({
        stdin: { contents: transformed, loader: name === 'view' ? 'js' : 'tsx' },
        bundle: true,
        format: 'cjs',
        platform: 'node',
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'native-ports',
            setup(builder) {
              builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
            },
          },
        ],
      });
      return [name, result.outputFiles[0]!.text] as const;
    }),
  ),
);
type Element = { type: unknown; props: Record<string, any> };
function nodes(value: any): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  return value && typeof value === 'object' && 'props' in value
    ? [value, ...nodes(value.props.children)]
    : [];
}
function text(value: any): string {
  return typeof value === 'string'
    ? value
    : Array.isArray(value)
      ? value.map(text).join(' ')
      : value && typeof value === 'object' && 'props' in value
        ? text(value.props.children)
        : '';
}
async function settle() {
  for (let n = 0; n < 16; ++n) await Promise.resolve();
}
function fixture(locale: 'en' | 'pt-BR' = 'en') {
  const localization = createLocalization({ reviewRenewal }, [locale]);
  let now = Date.now();
  let nextTimer = 0;
  const timers = new Map<number, { at: number; fn: () => void }>();
  const compilerCaches: unknown[][] = [];
  let cacheCursor = 0,
    stateCursor = 0;
  let resultMessage: string | null = null;
  const state: any = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        registered: true,
        unlocked: true,
        setup_pending: false,
        identity_id: 'owner',
        device_id: 'phone',
        relay_url: 'https://relay.test',
      },
    },
  };
  let rows: any[] = [
    {
      id: 'old-request',
      agent: { id: 'agent', is_mine: true },
      status: 'Approved',
      tool_approval: {
        content_hash: 'ab'.repeat(32),
        artifact_id: 'fresh-item',
        artifact_title: 'Page',
        version: 1,
        operation: 'Replace block',
        expires_ms: '1',
        review: {
          state: 'Available',
          root_request_id: 'old-request',
          predecessor_request_id: null,
          successor_request_id: null,
        },
      },
    },
  ];
  const writes: any[] = [],
    routes: any[] = [];
  const listeners = new Set<() => void>(),
    canonical = new Set<() => void>(),
    preflight = new Set<() => void>();
  const foreground = new Set<(next: string) => void>(),
    cleanups: (() => void)[] = [];
  let read = async () => structuredClone(rows);
  const store = {
    getSnapshot: () => state,
    sessionEpoch: 1,
    isSessionRetiring: false,
    driver: { request: () => read() },
    request: async (command: any) => {
      writes.push(command);
      rows[0].tool_approval.review.state = 'Requested';
      return {};
    },
    subscribe(fn: () => void) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    subscribeCanonicalChanges(fn: () => void) {
      canonical.add(fn);
      return () => canonical.delete(fn);
    },
    registerSessionPreflight(fn: () => void) {
      preflight.add(fn);
      return () => preflight.delete(fn);
    },
  };
  const port = flow.createReviewPort(store as never);
  const session = new flow.ToolReviewSession(port, 'old-request');
  session.activate(port.scope());
  let displayedSession = session;
  function load(name: keyof typeof files) {
    const module = { exports: {} as any };
    const require = (path: string): any => {
      if (path === 'react/jsx-runtime')
        return {
          jsx: (type: unknown, props: unknown) => ({ type, props }),
          jsxs: (type: unknown, props: unknown) => ({ type, props }),
          Fragment: 'Fragment',
        };
      if (path === 'react/compiler-runtime')
        return {
          c(size: number) {
            return (compilerCaches[cacheCursor++] ??= Array(size).fill(
              Symbol.for('react.memo_cache_sentinel'),
            ));
          },
        };
      if (path === 'react')
        return {
          useRef: (value: unknown) => ({ current: value }),
          useState: (value: unknown) => [stateCursor++ === 2 ? resultMessage : value, () => {}],
          useMemo: (fn: () => unknown) => fn(),
          useCallback: (fn: unknown) => fn,
          useSyncExternalStore: (_s: unknown, get: () => unknown) => get(),
        };
      if (path === 'react-native')
        return {
          Text: 'Text',
          View: 'View',
          Pressable: 'Pressable',
          AppState: {
            currentState: 'active',
            addEventListener(_name: string, fn: (next: string) => void) {
              foreground.add(fn);
              return { remove: () => foreground.delete(fn) };
            },
          },
        };
      if (path === 'expo-router')
        return {
          router: { push: (route: unknown) => routes.push(route) },
          useFocusEffect: (fn: () => () => void) => cleanups.push(fn()),
        };
      if (path.includes('i18n/index'))
        return {
          t: (key: string) =>
            key.startsWith('reviewRenewal.')
              ? localization.t(key as `reviewRenewal.${keyof typeof reviewRenewal.en}`)
              : key,
          formatLocale: 'en',
        };
      if (path.endsWith('/primitives'))
        return { NativeButton: 'NativeButton', Avatar: 'Avatar', Surface: 'Surface' };
      if (path.endsWith('/dialogs'))
        return {
          useDialogs: () => ({
            confirm: () => {
              throw new Error('Renewal must not add a confirmation dialog');
            },
          }),
        };
      if (path.endsWith('/presentation')) return { money: () => '' };
      if (path.endsWith('/tool-request-decision'))
        return {
          toolApprovalExpired: (tool: any) => Number(tool.expires_ms) <= now,
          toolApprovalValid: () => true,
        };
      if (path.endsWith('/tool-review-recovery'))
        return { ToolReviewRecovery: 'ToolReviewRecovery' };
      if (path.endsWith('/theme')) return { useTheme: () => ({ colors: { secondary: '#888' } }) };
      if (path.endsWith('/use-tool-review'))
        return {
          useToolReview: () => ({
            session: displayedSession,
            state: displayedSession.getSnapshot(),
          }),
        };
      if (path.endsWith('/core/provider')) return { useClient: () => ({ store }) };
      if (path.endsWith('/tool-review-renewal.ts')) return flow;
      throw new Error(`Unexpected fixture import ${path}`);
    };
    runInNewContext(compiled.get(name)!, {
      module,
      exports: module.exports,
      require,
      Date: class extends Date {
        static now() {
          return now;
        }
      },
      setTimeout: (fn: () => void, ms: number) => {
        const id = ++nextTimer;
        timers.set(id, { at: now + ms, fn });
        return id;
      },
      clearTimeout: (id: number) => timers.delete(id),
    });
    return module.exports;
  }
  return {
    state,
    store,
    session,
    writes,
    routes,
    foreground,
    listeners,
    canonical,
    preflight,
    rows: (next: any[]) => {
      rows = next;
    },
    base: () => structuredClone(rows),
    reads: (fn: typeof read) => {
      read = fn;
    },
    view: () => {
      cacheCursor = 0;
      return load('view').ToolReviewRecovery({ requestId: 'old-request' });
    },
    card: (request: any, result: string | null = null) => {
      stateCursor = 0;
      resultMessage = result;
      return load('card').RequestCard({ request });
    },
    clock: () => now,
    timerCount: () => timers.size,
    advance: async (ms: number) => {
      now += ms;
      for (const [id, timer] of timers)
        if (timer.at <= now) {
          timers.delete(id);
          timer.fn();
        }
      await settle();
    },
    hook: () => {
      const hook = load('hook').useToolReview('old-request');
      displayedSession = hook.session;
      return hook;
    },
    close: () => cleanups.forEach((fn) => fn()),
  };
}

test('compiled native review button directly requests one inert intent, never approval or action', async () => {
  for (const locale of ['en', 'pt-BR'] as const) {
    const f = fixture(locale);
    await f.session.refresh();
    const tree = f.view();
    const button = nodes(tree).find((node) => node.type === 'NativeButton')!;
    assert.equal(
      button.props.label,
      locale === 'en' ? 'Request a new review' : 'Solicitar nova revisão',
    );
    assert.ok(
      text(tree).includes(
        locale === 'en' ? 'approval period ended' : 'prazo da aprovação terminou',
      ),
    );
    button.props.onPress();
    await settle();
    assert.deepEqual(f.writes, [
      {
        op: 'decide_tool_request',
        request_id: 'old-request',
        observed_content_hash: 'ab'.repeat(32),
        decision: 'RequestReview',
      },
    ]);
    assert.ok(
      text(f.view()).includes(locale === 'en' ? 'New review requested' : 'Nova revisão solicitada'),
    );
    assert.ok(nodes(f.view()).every((node) => node.props.label !== button.props.label));
  }
});

test('compiled successor button rechecks current scope before opening the exact new request', async () => {
  const f = fixture();
  const rows = f.base();
  rows[0].tool_approval.review.state = 'Superseded';
  rows[0].tool_approval.review.successor_request_id = 'new-request';
  f.rows(rows);
  await f.session.refresh();
  const button = nodes(f.view()).find((node) => node.props.label === 'Open the new request')!;
  button.props.onPress();
  assert.equal(f.routes[0].params.id, 'new-request');
  f.state.snapshot.account.device_id = 'replacement';
  button.props.onPress();
  assert.equal(f.routes.length, 1);
});

test('actual compiled hook retires preflight reads on background/account and reloads only on focus/foreground', async () => {
  const f = fixture();
  const hook = f.hook();
  await settle();
  assert.ok(hook.session.ticket());
  let finish!: (value: any[]) => void;
  f.reads(
    () =>
      new Promise<any[]>((yes) => {
        finish = yes;
      }),
  );
  const attempt = hook.session.renew(hook.session.ticket());
  for (const fn of f.foreground) fn('background');
  finish(f.base());
  assert.equal(await attempt, false);
  assert.equal(f.writes.length, 0);
  f.reads(async () => f.base());
  for (const fn of f.foreground) fn('active');
  await settle();
  assert.ok(hook.session.ticket());
  f.state.snapshot.account.identity_id = 'different-owner';
  for (const fn of f.listeners) fn();
  assert.equal(hook.session.ticket(), null);
  f.close();
  assert.equal(f.foreground.size, 0);
  assert.equal(f.listeners.size, 0);
  assert.equal(f.canonical.size, 0);
  assert.equal(f.preflight.size, 0);
});

test('actual React compiler updates a stable session from cold null through eligibility and successor', async () => {
  const f = fixture();
  assert.ok(nodes(f.view()).every((n) => n.props.label !== 'Request a new review'));
  await f.session.refresh();
  assert.ok(nodes(f.view()).some((n) => n.props.label === 'Request a new review'));
  const prior = f.session.getSnapshot();
  const rows = f.base();
  rows[0].tool_approval.review.state = 'Superseded';
  rows[0].tool_approval.review.successor_request_id = 'next';
  f.rows(rows);
  await f.session.refresh();
  assert.equal(f.session.ticket(prior), null);
  assert.equal(f.session.successor(prior), null);
  assert.ok(nodes(f.view()).some((n) => n.props.label === 'Open the new request'));
  assert.ok(nodes(f.view()).every((n) => n.props.label !== 'Request a new review'));
});

test('focused foreground expiry performs one read-only metadata refresh; inactive or swapped scope cancels it', async (t) => {
  for (const retire of ['none', 'background', 'account', 'blur']) {
    t.mock.restoreAll();
    const f = fixture();
    t.mock.method(Date, 'now', () => f.clock());
    const rows = f.base();
    rows[0].tool_approval.expires_ms = String(f.clock() + 1000);
    rows[0].tool_approval.review.state = 'Unavailable';
    f.rows(rows);
    let reads = 0;
    f.reads(async () => {
      ++reads;
      const current = structuredClone(rows);
      if (f.clock() >= Number(current[0].tool_approval.expires_ms))
        current[0].tool_approval.review.state = 'Available';
      return current;
    });
    f.hook();
    await settle();
    assert.equal(reads, 1);
    assert.equal(f.timerCount(), 1);
    assert.equal(f.view(), null);
    if (retire === 'background') for (const fn of f.foreground) fn('background');
    if (retire === 'account') {
      f.state.snapshot.account.device_id = 'replacement';
      for (const fn of f.listeners) fn();
    }
    if (retire === 'blur') f.close();
    await f.advance(1001);
    assert.equal(reads, retire === 'none' ? 2 : 1);
    assert.equal(f.writes.length, 0);
    assert.equal(f.timerCount(), 0);
    if (retire === 'none')
      assert.ok(nodes(f.view()).some((n) => n.props.label === 'Request a new review'));
    f.close();
  }
});

test('compiled approved card distinguishes historical approval from expired/requested/superseded/reconcile state', async () => {
  for (const state of ['Available', 'Requested', 'Superseded', 'Reconcile']) {
    const f = fixture();
    const rows = f.base();
    rows[0].tool_approval.review.state = state;
    if (state === 'Superseded') rows[0].tool_approval.review.successor_request_id = 'next';
    f.rows(rows);
    await f.session.refresh();
    const tree = f.card(rows[0], 'Approved once. Waiting for the reviewed action.');
    assert.ok(text(tree).includes('Previous decision: approved once.'));
    assert.ok(!text(tree).includes('Waiting for the reviewed action.'));
    assert.ok(nodes(tree).some((n) => n.type === 'ToolReviewRecovery'));
  }
});
