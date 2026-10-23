import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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
const compiled = new Map(
  await Promise.all(
    Object.entries(files).map(async ([name, path]) => {
      const result = await build({
        stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
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
    routes: any[] = [],
    confirmations: any[] = [];
  const listeners = new Set<() => void>(),
    canonical = new Set<() => void>(),
    preflight = new Set<() => void>();
  const foreground = new Set<(next: string) => void>(),
    cleanups: (() => void)[] = [];
  let confirm: () => Promise<boolean> = async () => false;
  const store = {
    getSnapshot: () => state,
    sessionEpoch: 1,
    isSessionRetiring: false,
    driver: { request: async () => structuredClone(rows) },
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
  function load(name: keyof typeof files) {
    const module = { exports: {} as any };
    const require = (path: string): any => {
      if (path === 'react/jsx-runtime')
        return {
          jsx: (type: unknown, props: unknown) => ({ type, props }),
          jsxs: (type: unknown, props: unknown) => ({ type, props }),
          Fragment: 'Fragment',
        };
      if (path === 'react')
        return {
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
      if (path.includes('i18n/index')) return { t: localization.t };
      if (path.endsWith('/primitives')) return { NativeButton: 'NativeButton' };
      if (path.endsWith('/dialogs'))
        return {
          useDialogs: () => ({
            confirm: (options: unknown) => {
              confirmations.push(options);
              return confirm();
            },
          }),
        };
      if (path.endsWith('/theme')) return { useTheme: () => ({ colors: { secondary: '#888' } }) };
      if (path.endsWith('/use-tool-review'))
        return { useToolReview: () => ({ session, state: session.getSnapshot() }) };
      if (path.endsWith('/core/provider')) return { useClient: () => ({ store }) };
      if (path.endsWith('/tool-review-renewal.ts')) return flow;
      throw new Error(`Unexpected fixture import ${path}`);
    };
    runInNewContext(compiled.get(name)!, { module, exports: module.exports, require });
    return module.exports;
  }
  return {
    state,
    store,
    session,
    writes,
    routes,
    confirmations,
    foreground,
    listeners,
    canonical,
    preflight,
    rows: (next: any[]) => {
      rows = next;
    },
    base: () => structuredClone(rows),
    confirm: (fn: () => Promise<boolean>) => {
      confirm = fn;
    },
    view: () => load('view').ToolReviewRecovery({ requestId: 'old-request' }),
    hook: () => load('hook').useToolReview('old-request'),
    close: () => cleanups.forEach((fn) => fn()),
  };
}

test('compiled native review explains renewal and emits no approval or action until confirmed', async () => {
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
    assert.equal(f.confirmations.length, 1);
    assert.equal(f.writes.length, 0);
    assert.ok(
      f.confirmations[0].message.includes(
        locale === 'en' ? 'does not approve or repeat' : 'não aprova nem repete',
      ),
    );
    f.confirm(async () => true);
    nodes(f.view())
      .find((node) => node.type === 'NativeButton')!
      .props.onPress();
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

test('actual compiled hook retires dialogs on background/account and reloads only on focus/foreground', async () => {
  const f = fixture();
  const hook = f.hook();
  await settle();
  assert.ok(hook.session.ticket());
  let confirm!: (value: boolean) => void;
  const attempt = hook.session.renew(
    hook.session.ticket(),
    () =>
      new Promise<boolean>((yes) => {
        confirm = yes;
      }),
  );
  for (const fn of f.foreground) fn('background');
  confirm(true);
  assert.equal(await attempt, false);
  assert.equal(f.writes.length, 0);
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
