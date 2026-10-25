import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { constants, runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { CanonicalDrawingView } from './canonical-view';
import { frameClosures } from './lifetime.ts';

type Element = { type: unknown; props: Record<string, unknown> };
type Effect = () => void | (() => void);
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
const exportsByPort: Record<string, string[]> = {
  react: ['useState', 'useRef', 'useCallback', 'useMemo', 'useEffect', 'useLayoutEffect'],
  'react/jsx-runtime': ['jsx', 'jsxs'],
  'expo-router': ['router', 'useFocusEffect'],
  'react-native': ['AppState', 'Linking', 'Text', 'View'],
  'react-native-safe-area-context': ['useSafeAreaInsets'],
  'expo-crypto': ['randomUUID', 'CryptoDigestAlgorithm', 'CryptoEncoding', 'digestStringAsync'],
  '../core/provider': ['useClient'],
  '../components/primitives': ['Loading', 'NativeButton', 'Surface'],
  '../components/dialogs': ['useDialogs'],
  '../theme': ['useTheme'],
  '../i18n/index.ts': ['locale', 't'],
  './frame': ['MiniappFrame'],
  './app-sheet-context': ['useAppSheetCloseButton'],
};
const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('./canonical-view.tsx', import.meta.url), 'utf8'),
    resolveDir: new URL('.', import.meta.url).pathname,
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  define: { 'process.env.EXPO_OS': '"android"' },
  plugins: [
    {
      name: 'component-framework-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          path in exportsByPort ? { path, namespace: 'component-port' } : undefined,
        );
        builder.onLoad({ filter: /.*/, namespace: 'component-port' }, ({ path }) => ({
          contents:
            `const p=require('resource-test-ports')[${JSON.stringify(path)}];\n` +
            exportsByPort[path]!.map((name) => `export const ${name}=p.${name};`).join('\n'),
        }));
        builder.onResolve({ filter: /^resource-test-ports$/ }, ({ path }) => ({
          path,
          external: true,
        }));
      },
    },
  ],
});
const require = createRequire(import.meta.url);
const tick = async () => {
  for (let i = 0; i < 32; i++) await Promise.resolve();
};
function fixture(owned = false) {
  let current!: { slots: Slot[]; cursor: number; effects: (() => void)[]; focus?: Effect };
  let hashes = 0,
    preflights = 0,
    retired = 0;
  let held: { entered(): void; wait: Promise<void> } | undefined;
  const slot = () => (current.slots[current.cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    !!a && !!b && a.length === b.length && a.every((value, i) => Object.is(value, b[i]));
  const memo = (read: () => unknown, deps: readonly unknown[]) => {
    const at = slot();
    if (!same(at.deps, deps)) {
      at.deps = deps;
      at.value = read();
    }
    return at.value;
  };
  const effect = (read: Effect, deps?: readonly unknown[]) => {
    const at = slot();
    if (same(at.deps, deps)) return;
    at.deps = deps;
    current.effects.push(() => {
      at.cleanup?.();
      at.cleanup = read() ?? undefined;
    });
  };
  const scene = '{"elements":[]}';
  const html = '<html><head></head><body>Drawing</body></html>';
  const uri = owned ? `ui://zoen/package/file/1/${'a'.repeat(64)}` : 'ui://zoen/excalidraw.html';
  const commands: { op: string; expected_resource_uri?: string }[] = [];
  const store = {
    sessionEpoch: 1,
    getSnapshot: () => ({
      mode: 'account',
      phase: 'ready',
      erasePending: false,
      snapshot: {
        account: {
          identity_id: 'owner',
          device_id: 'device',
          unlocked: true,
          setup_pending: false,
        },
        conversations: [{ id: 'space' }],
      },
    }),
    driver: {
      request: async (request: { op: string; expected_resource_uri?: string }) => {
        commands.push(request);
        if (request.op === 'app_resource' || request.op === 'app_document_resource')
          return {
            uri,
            mime_type: 'text/html;profile=mcp-app',
            text: html,
            prefers_border: false,
            manifest_json: JSON.stringify({
              id: 'excalidraw',
              name: 'Drawing',
              capabilities: [],
              allowedDomains: [],
              sha256: createHash('sha256').update(html).digest('hex'),
              bytes: Buffer.byteLength(html),
            }),
          };
        if (request.op === 'app_document_read' || request.op === 'app_document_read_pinned')
          return {
            item_id: 'item',
            version: 1,
            current_version: 1,
            scene_json: scene,
            sha256: createHash('sha256').update(scene).digest('hex'),
          };
        throw new Error(`Unexpected request ${request.op}`);
      },
    },
    subscribe: () => () => {},
    registerSessionPreflight: () => {
      preflights++;
      return () => {
        retired++;
      };
    },
    registerSessionCleanup: () => () => {},
  };
  const theme = { colors: { background: '#000', text: '#fff' }, scheme: 'dark' };
  const dialogs = {};
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const ports: Record<string, unknown> = {
    './app-sheet-context': { useAppSheetCloseButton: () => null },
    react: {
      useState: (initial: unknown) => {
        const at = slot();
        if (!Object.hasOwn(at, 'value'))
          at.value = typeof initial === 'function' ? initial() : initial;
        return [
          at.value,
          (next: unknown) => {
            at.value = typeof next === 'function' ? next(at.value) : next;
          },
        ];
      },
      useRef: (initial: unknown) => {
        const at = slot();
        at.value ??= { current: initial };
        return at.value;
      },
      useCallback: (read: unknown, deps: readonly unknown[]) => memo(() => read, deps),
      useMemo: memo,
      useEffect: effect,
      useLayoutEffect: effect,
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'expo-router': {
      router: { back() {} },
      useFocusEffect: (read: Effect) => {
        current.focus = read;
      },
    },
    'react-native': {
      View: 'View',
      Text: 'Text',
      Linking: {},
      AppState: { addEventListener: () => ({ remove() {} }) },
    },
    'react-native-safe-area-context': {
      useSafeAreaInsets: () => ({ top: 0, right: 0, bottom: 0, left: 0 }),
    },
    'expo-crypto': {
      randomUUID: () => '00000000-0000-4000-8000-000000000001',
      CryptoDigestAlgorithm: { SHA256: 'SHA-256' },
      CryptoEncoding: { HEX: 'hex' },
      digestStringAsync: async (_algorithm: string, text: string) => {
        hashes++;
        if (held) {
          const gate = held;
          held = undefined;
          gate.entered();
          await gate.wait;
        }
        return createHash('sha256').update(text).digest('hex');
      },
    },
    '../core/provider': { useClient: () => ({ store }) },
    '../components/primitives': {
      Loading: 'Loading',
      NativeButton: 'NativeButton',
      Surface: 'Surface',
    },
    '../components/dialogs': { useDialogs: () => dialogs },
    '../theme': { useTheme: () => theme },
    '../i18n/index.ts': { locale: 'en', t: (key: string) => key },
    './frame': { MiniappFrame: 'MiniappFrame' },
  };
  const module = { exports: {} as { CanonicalDrawingView: typeof CanonicalDrawingView } };
  runInNewContext(
    compiled.outputFiles[0]!.text,
    {
      module,
      exports: module.exports,
      require: (name: string) => (name === 'resource-test-ports' ? ports : require(name)),
      setTimeout,
      clearTimeout,
      setInterval,
      clearInterval,
      TextEncoder,
      TextDecoder,
      URL,
      AbortController,
      structuredClone,
      console,
    },
    { importModuleDynamically: constants.USE_MAIN_CONTEXT_DEFAULT_LOADER },
  );
  const mount = (component: (props: never) => unknown, props: unknown) => {
    const hooks = {
      slots: [] as Slot[],
      cursor: 0,
      effects: [] as (() => void)[],
      focus: undefined as Effect | undefined,
    };
    let blur: (() => void) | undefined;
    return {
      render() {
        current = hooks;
        hooks.cursor = 0;
        hooks.effects = [];
        const value = component(props as never) as Element | null;
        for (const commit of hooks.effects) commit();
        return value;
      },
      focus() {
        blur = hooks.focus?.() ?? undefined;
      },
      blur() {
        blur?.();
        blur = undefined;
      },
      unmount() {
        blur?.();
        for (const at of hooks.slots) at.cleanup?.();
      },
    };
  };
  const item = {
    id: 'item',
    space_id: 'space',
    title: 'Drawing',
    app: owned ? { app_id: 'excalidraw', resource_uri: uri } : undefined,
  };
  const wrapper = mount(module.exports.CanonicalDrawingView, {
    item,
    spec: { id: 'excalidraw', resource_uri: 'ui://zoen/excalidraw.html' },
  });
  return {
    wrapper,
    mount,
    commands,
    changePin: (next: string) => {
      if (item.app) item.app.resource_uri = next;
    },
    counts: () => ({ hashes, preflights, retired }),
    hold() {
      let release!: () => void, enter!: () => void;
      const wait = new Promise<void>((resolve) => {
        release = resolve;
      });
      const entered = new Promise<void>((resolve) => {
        enter = resolve;
      });
      held = { wait, entered: enter };
      return { release, entered };
    },
  };
}

test('actual owned drawing pins both Core reads and keeps its mounted resource when another device changes release', async () => {
  const app = fixture(true);
  app.wrapper.render();
  app.wrapper.focus();
  const focused = app.wrapper.render()!;
  const child = app.mount(focused.type as (props: never) => unknown, focused.props);
  child.render();
  await tick();
  const first = child.render()!;
  assert.equal(typeof first.type, 'function');
  const uri = `ui://zoen/package/file/1/${'a'.repeat(64)}`;
  assert.deepEqual(
    app.commands.map((request) => [request.op, request.expected_resource_uri]),
    [
      ['app_document_resource', uri],
      ['app_document_read_pinned', uri],
    ],
  );
  app.changePin(`ui://zoen/package/file/2/${'b'.repeat(64)}`);
  const second = child.render()!;
  assert.equal(second.props.verified, first.props.verified, 'no resource reload or replacement');
  assert.equal(second.props.doc, first.props.doc, 'no replacement of the editor draft/base');
  assert.equal(
    app.commands.length,
    2,
    'the live resource never follows mutable spec/item metadata',
  );
  assert.equal(app.counts().hashes, 1);
  child.unmount();
  app.wrapper.unmount();
});

test('compiled drawing wrapper cancels pending verification synchronously on blur before child React cleanup', async () => {
  const app = fixture();
  const gate = app.hold();
  app.wrapper.render();
  app.wrapper.focus();
  const focused = app.wrapper.render()!;
  const child = app.mount(focused.type as (props: never) => unknown, focused.props);
  assert.equal(child.render()!.type, 'Loading');
  await gate.entered;
  app.wrapper.blur();
  gate.release();
  await tick();
  assert.equal(child.render()!.type, 'Loading');
  assert.deepEqual(app.counts(), { hashes: 1, preflights: 0, retired: 0 });
  child.unmount();
  app.wrapper.unmount();
});

test('compiled drawing uses one verified pair through host mount and closes the host on blur without child cleanup', async () => {
  const app = fixture();
  app.wrapper.render();
  app.wrapper.focus();
  const focused = app.wrapper.render()!;
  const child = app.mount(focused.type as (props: never) => unknown, focused.props);
  child.render();
  await tick();
  const ready = child.render()!;
  assert.equal(typeof ready.type, 'function');
  const document = app.mount(ready.type as (props: never) => unknown, ready.props);
  document.render();
  document.render();
  await tick();
  assert.deepEqual(app.counts(), { hashes: 1, preflights: 1, retired: 0 });
  app.wrapper.blur();
  assert.deepEqual(app.counts(), { hashes: 1, preflights: 1, retired: 1 });
  document.unmount();
  child.unmount();
  app.wrapper.unmount();
});

test('old loader cleanup cannot detach its document or enter a newer focus group', () => {
  const old = frameClosures(),
    next = frameClosures();
  let documents = 0,
    loaders = 0,
    replacements = 0;
  const removeLoader = old.register(() => {
    loaders++;
  });
  old.register(() => {
    documents++;
  });
  next.register(() => {
    replacements++;
  });
  removeLoader();
  old.close();
  old.close();
  assert.deepEqual(
    { documents, loaders, replacements },
    { documents: 1, loaders: 0, replacements: 0 },
  );
  old.register(() => {
    loaders++;
  });
  assert.equal(loaders, 1, 'a delayed registration retains its already closed original focus');
  assert.equal(replacements, 0);
  next.close();
  assert.equal(replacements, 1);
});

test('focus cleanup attempts every closer even if one throws', () => {
  const focus = frameClosures();
  let retired = false;
  focus.register(() => {
    throw new Error('failed teardown');
  });
  focus.register(() => {
    retired = true;
  });
  assert.throws(focus.close, /failed teardown/);
  assert.equal(retired, true);
  focus.close();
});
