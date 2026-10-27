import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as flow from './tool-review-renewal.ts';
import * as ownership from './account-scope.ts';
import * as siteSource from '../sites/source.ts';
import { createLocalization } from '../i18n/runtime.ts';
import { reviewRenewal } from '../i18n/catalogs/review-renewal.ts';
import { imageToolReviewValid } from './image-tool-review.ts';
import { imageReview } from '../i18n/catalogs/image-review.ts';
import * as decision from './tool-request-decision.ts';
import * as appReview from '../miniapps/app-request-review.ts';
import * as documentAuthority from '../documents/document-authority.ts';

const files = {
  view: '../agents/tool-review-recovery.tsx',
  hook: '../agents/use-tool-review.ts',
  card: '../agents/request-card.tsx',
  lease: '../agents/use-request-decision-lease.ts',
  site: '../agents/site-tool-review.tsx',
  action: '../agents/app-request-review-action.tsx',
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
      const useCompiler = name !== 'site';
      const diagnostics: string[] = [];
      const transformed = useCompiler
        ? transformSync(source, {
            filename: fileURLToPath(new URL(path, import.meta.url)),
            configFile: false,
            babelrc: false,
            presets: [require.resolve('babel-preset-expo')],
            plugins: [
              [
                'babel-plugin-react-compiler',
                {
                  target: '19',
                  logger: {
                    logEvent(
                      _file: unknown,
                      event: { kind: string; detail?: { options?: { reason?: string } } },
                    ) {
                      if (event.kind === 'CompileError' || event.kind === 'CompileDiagnostic')
                        diagnostics.push(event.detail?.options?.reason ?? event.kind);
                    },
                  },
                },
              ],
            ],
            caller: {
              name: 'metro',
              platform: 'ios',
              engine: 'hermes',
              isDev: false,
              supportsStaticESM: false,
            },
          }).code
        : source;
      // RequestCard already falls back on b260 because its existing async
      // decision handler uses try/finally. Exercise that normal output without
      // hiding additional diagnostics or changing unrelated decision behavior.
      assert.deepEqual(
        diagnostics,
        name === 'card'
          ? ["(BuildHIR::lowerStatement) Handle TryStatement with a finalizer ('finally') clause"]
          : [],
      );
      if (name === 'view' || name === 'hook') assert.match(transformed, /react\/compiler-runtime/);
      const result = await build({
        stdin: { contents: transformed, loader: useCompiler ? 'js' : 'tsx' },
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
export function nodes(value: any): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  return value && typeof value === 'object' && 'props' in value
    ? [value, ...nodes(value.props.children)]
    : [];
}
export function text(value: any): string {
  return typeof value === 'string'
    ? value
    : Array.isArray(value)
      ? value.map(text).join(' ')
      : value && typeof value === 'object' && 'props' in value
        ? text(value.props.children)
        : '';
}
export async function settle() {
  for (let n = 0; n < 16; ++n) await Promise.resolve();
}
export function fixture(locale: 'en' | 'pt-BR' = 'en') {
  const localization = createLocalization({ reviewRenewal, imageReview }, [locale]);
  let now = Date.now();
  let nextTimer = 0;
  const timers = new Map<number, { at: number; fn: () => void }>();
  const compilerCaches = new Map<keyof typeof files, unknown[][]>();
  let cacheCursor = 0,
    stateCursor = 0;
  let resultMessage: string | null = null;
  const state: any = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      conversations: [{ id: 'site-space', members: [{ id: 'owner' }] }],
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
  const sessionCleanup = new Set<() => Promise<void>>();
  const foreground = new Set<(next: string) => void>(),
    cleanups: (() => void)[] = [];
  let read = async () => structuredClone(rows);
  const siteState: unknown[] = [],
    siteRefs: { current: any }[] = [];
  let settlingCard = false;
  let siteMounted = false,
    siteRefCursor = 0;
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
    registerSessionCleanup(fn: () => Promise<void>) {
      sessionCleanup.add(fn);
      return () => sessionCleanup.delete(fn);
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
            let caches = compilerCaches.get(name);
            if (!caches) compilerCaches.set(name, (caches = []));
            return (caches[cacheCursor++] ??= Array(size).fill(
              Symbol.for('react.memo_cache_sentinel'),
            ));
          },
        };
      if (path === 'react')
        return {
          useRef: (value: unknown) =>
            name === 'site'
              ? (siteRefs[siteRefCursor++] ??= { current: value })
              : { current: value },
          useState: (value: unknown) => {
            const index = stateCursor++;
            if (name !== 'site') return [index === 2 ? resultMessage : value, () => {}];
            if (!(index in siteState)) siteState[index] = value;
            return [
              siteState[index],
              (next: unknown) => {
                siteState[index] = next;
              },
            ];
          },
          useMemo: (fn: () => unknown) => fn(),
          useLayoutEffect: (fn: () => void) => fn(),
          useCallback: (fn: unknown) => fn,
          useSyncExternalStore: (_s: unknown, get: () => unknown) => get(),
        };
      if (path === 'react-native')
        return {
          Platform: { OS: 'ios' },
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
          useFocusEffect: (fn: () => () => void) => {
            if (name === 'lease' && settlingCard) return;
            if (name === 'site') {
              if (siteMounted) return;
              siteMounted = true;
            }
            cleanups.push(fn());
          },
        };
      if (path === '../documents/document-authority.ts') return documentAuthority;
      if (path === './use-request-decision-lease') return load('lease');
      if (path === '../i18n' || path.includes('i18n/index'))
        return {
          t: (key: string) =>
            key.startsWith('reviewRenewal.')
              ? localization.t(key as `reviewRenewal.${keyof typeof reviewRenewal.en}`)
              : key.startsWith('imageReview.')
                ? localization.t(key as `imageReview.${keyof typeof imageReview.en}`)
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
      if (path.endsWith('/image-tool-review')) return { imageToolReviewValid };
      if (path.endsWith('/tool-request-decision'))
        return {
          ...decision,
          toolApprovalExpired: (tool: Parameters<typeof decision.toolApprovalExpired>[0]) =>
            decision.toolApprovalExpired(tool, now),
          requestDecisionCommand: (...args: Parameters<typeof decision.requestDecisionCommand>) =>
            decision.requestDecisionCommand(args[0], args[1], now),
        };
      if (path.endsWith('/ZoenCoreModule')) return { sitePreviewVersion: 2 };
      if (path.endsWith('/account-scope.ts')) return ownership;
      if (path.endsWith('/drawing-tool-review')) return { DrawingToolReview: 'DrawingToolReview' };
      if (path.endsWith('/app-request-review')) return appReview;
      if (path.endsWith('/app-request-review-action')) return load('action');
      if (path.endsWith('/sites/source')) return siteSource;
      if (path.endsWith('/sites/preview')) return { SitePreview: 'SitePreview' };
      if (path.endsWith('/site-tool-review'))
        return { SiteToolReview: load('site').SiteToolReview };
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
      if (path.endsWith('/core/provider')) return { useClient: () => ({ store, ...state }) };
      if (path.endsWith('/core/account-scope')) return ownership;
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
    site: (ready: (value: boolean) => void) => {
      stateCursor = 0;
      siteRefCursor = 0;
      return load('site').SiteToolReview({
        requestId: 'site-request',
        contentHash: 'ab'.repeat(32),
        spaceId: 'site-space',
        onReady: ready,
      });
    },
    card: (request: any, result: string | null = null) => {
      cacheCursor = 0;
      stateCursor = 0;
      resultMessage = result;
      // Model the rerender scheduled by the real lease's mounted focus effect.
      // This static renderer does not run React's scheduler; lifecycle cases use
      // the real React/Expo/ClientStore fixture instead.
      load('card').RequestCard({ request });
      cacheCursor = 0;
      stateCursor = 0;
      settlingCard = true;
      try {
        return load('card').RequestCard({ request });
      } finally {
        settlingCard = false;
      }
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
