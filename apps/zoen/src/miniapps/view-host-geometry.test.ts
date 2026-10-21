import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
import { App } from '@modelcontextprotocol/ext-apps';
import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import { MiniappSession } from './protocol.ts';
import { frameAuthority, frameClosures } from './lifetime.ts';

const path = new URL('./view-host.tsx', import.meta.url);
const compiled = await build({
  stdin: {
    contents: readFileSync(path, 'utf8') + '\nexport { ReadyView };',
    sourcefile: fileURLToPath(path),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  define: { 'process.env.EXPO_OS': '"ios"' },
  plugins: [
    {
      name: 'geometry-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /.*/ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});

type Element = { type: unknown; props: Record<string, any> };
type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
  !!a && !!b && a.length === b.length && a.every((value, index) => Object.is(value, b[index]));
function find(tree: unknown, type: unknown): Element | undefined {
  if (Array.isArray(tree)) return tree.flatMap((child) => find(child, type) ?? [])[0];
  if (!tree || typeof tree !== 'object' || !('props' in tree)) return;
  const element = tree as Element;
  return element.type === type ? element : find(element.props.children, type);
}

async function fixture() {
  const slots: Slot[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  let live = true;
  let disposals = 0;
  let sessionCount = 0;
  const writes: unknown[] = [];
  const listeners = new Set<() => void>();
  const closes = frameClosures();
  const snapshot = { conversations: [{ id: 'chat', members: [] }] };
  const store = {
    subscribe: (callback: () => void) => {
      listeners.add(callback);
      return () => listeners.delete(callback);
    },
    request: async (input: unknown) => {
      writes.push(input);
      throw Error('Unexpected mutation');
    },
  };
  const consent = { ask: async () => 'deny', cancelAll: () => {}, current: null };
  const dialogs = { confirm: async () => false };
  const colors = {
    background: '#ffffff',
    surface: '#eeeeee',
    text: '#111111',
    secondary: '#555555',
    line: '#cccccc',
  };
  let scheme = 'light';
  let closeButton: { top: number; right: number; width: number; height: number } | null = null;
  let insets = { top: 107, right: 0, bottom: 34, left: 0 };
  const props = {
    item: {
      id: 'app-one',
      space_id: 'chat',
      title: 'Synthetic app',
      version: 7,
      app: { headline: 'Saved content', view_json: '{"text":"saved"}' },
    },
    spec: { resource_uri: 'ui://synthetic/app', tools: [] },
    fill: true,
    verified: {
      resource: {
        uri: 'ui://synthetic/app',
        mime_type: 'text/html;profile=mcp-app',
        text: '<!doctype html><p>Owned fixture</p>',
      },
      manifest: { name: 'Synthetic', capabilities: [], allowedDomains: [] },
    },
    checkOwner: () => {
      if (!live) throw Error('Owner changed');
    },
    networkHosts: '',
    onNetworkChange: () => {},
    registerClose: closes.register,
    onReload: () => {},
  };
  const slot = () => (slots[cursor++] ??= {});
  const memo = (calculate: () => unknown, deps: readonly unknown[]) => {
    const s = slot();
    if (!same(s.deps, deps)) {
      s.value = calculate();
      s.deps = deps;
    }
    return s.value;
  };
  const effect = (read: () => (() => void) | void, deps: readonly unknown[]) => {
    const s = slot();
    if (!same(s.deps, deps)) {
      s.deps = deps;
      effects.push(() => {
        s.cleanup?.();
        s.cleanup = read() ?? undefined;
      });
    }
  };
  const ports: Record<string, unknown> = {
    react: {
      useState: (initial: unknown) => {
        const s = slot();
        if (!Object.hasOwn(s, 'value')) s.value = initial;
        return [
          s.value,
          (next: any) => {
            s.value = typeof next === 'function' ? next(s.value) : next;
          },
        ];
      },
      useRef: (initial: unknown) => {
        const s = slot();
        return (s.value ??= { current: initial });
      },
      useMemo: memo,
      useCallback: (callback: () => unknown, deps: readonly unknown[]) =>
        memo(() => callback, deps),
      useLayoutEffect: effect,
      useEffect: effect,
    },
    'react/jsx-runtime': {
      jsx: (type: unknown, props: Record<string, unknown>) => ({ type, props }),
      jsxs: (type: unknown, props: Record<string, unknown>) => ({ type, props }),
    },
    'expo-router': { router: { back: () => {} } },
    'react-native': { View: 'View', Text: 'Text' },
    'react-native-safe-area-context': { useSafeAreaInsets: () => insets },
    'expo-haptics': {},
    '../i18n/index.ts': { t: (key: string) => key, locale: 'en' },
    '../core/provider': { useClient: () => ({ store, snapshot }) },
    '../core/query': {},
    '../components/primitives': { NativeButton: 'NativeButton' },
    '../components/dialogs': { useDialogs: () => dialogs },
    '../theme': { useTheme: () => ({ colors, scheme }) },
    './frame': { MiniappFrame: 'MiniappFrame' },
    './app-sheet-context': { useAppSheetCloseButton: () => closeButton },
    './app-keyboard-viewport': { AppKeyboardViewport: 'AppKeyboardViewport' },
    './protocol': {
      MiniappSession: class extends MiniappSession {
        constructor(host: ConstructorParameters<typeof MiniappSession>[0]) {
          super(host);
          ++sessionCount;
        }
      },
    },
    './sandbox': { sandboxHtml: (html: string) => html },
    './resource-load': {},
    './json': { parseMiniappJson: JSON.parse },
    './actions': {},
    './photo-tokens': {},
    './native-capabilities': { available: () => [], releaseItemPhotos: () => {} },
    './network': {},
    './lifetime': { frameAuthority, frameClosures },
    './device-consent': { DeviceConsent: 'DeviceConsent' },
    './use-device-consent': { useDeviceConsent: () => consent },
    './canonical-view': {},
    './deckcraft-view': {},
  };
  const module = { exports: {} as { ReadyView: (props: unknown) => Element } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    Intl,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  let transport: Transport;
  let tree: Element;
  const render = () => {
    cursor = 0;
    effects = [];
    tree = module.exports.ReadyView(props);
    const frame = find(tree, 'MiniappFrame')!;
    frame.props.ref.current = {
      send: (message: JSONRPCMessage) => transport?.onmessage?.(message),
      dispose: () => {
        ++disposals;
      },
    };
    for (const commit of effects) commit();
    return frame;
  };
  const app = new App({ name: 'owned-geometry-app', version: '1' }, {}, { autoResize: false });
  const notifications: string[] = [];
  let draft = 'unsaved local edit';
  app.ontoolinput = () => notifications.push('input');
  app.ontoolresult = () => {
    notifications.push('result');
    draft = 'reset from canonical result';
  };
  transport = {
    start: async () => {},
    close: async () => {},
    send: async (message) => {
      renderFrame.props.onMessage(JSON.stringify(message));
    },
  };
  let renderFrame = render();
  await app.connect(transport);
  await tick();
  draft = 'unsaved local edit';
  return {
    app,
    props,
    notifications,
    writes,
    frame: () => renderFrame,
    sessionCount: () => sessionCount,
    disposals: () => disposals,
    draft: () => draft,
    async layout(width: number, height: number, nextInsets = insets) {
      insets = nextInsets;
      const wrapper = (tree.props.children as Element[]).find((child) => child?.props?.onLayout)!;
      wrapper.props.onLayout({ nativeEvent: { layout: { width, height } } });
      renderFrame = render();
      await tick();
    },
    async chrome(next: typeof closeButton) {
      closeButton = next;
      renderFrame = render();
      await tick();
    },
    async theme() {
      scheme = 'dark';
      renderFrame = render();
      await tick();
    },
    async content() {
      props.item = { ...props.item, version: 8 };
      renderFrame = render();
      await tick();
    },
    revoke() {
      live = false;
      for (const listener of listeners) listener();
    },
    async close() {
      closes.close();
      for (const s of slots) s.cleanup?.();
      await app.close();
    },
  };
}

test('compiled fullscreen App sends rotation geometry/insets through the real MCP bridge without resetting unsaved UI', async (t) => {
  const f = await fixture();
  t.after(f.close);
  assert.deepEqual(f.notifications, ['input', 'result']);
  const html = f.frame().props.html;
  const originalVersion = f.props.item.version;
  for (const [width, height, insets] of [
    [393, 852, { top: 107, right: 0, bottom: 34, left: 0 }],
    [852, 393, { top: 48, right: 59, bottom: 21, left: 59 }],
    [393, 852, { top: 107, right: 0, bottom: 34, left: 0 }],
  ] as const) {
    await f.layout(width, height, insets);
    assert.deepEqual(f.app.getHostContext()?.containerDimensions, { width, height });
    assert.deepEqual(f.app.getHostContext()?.safeAreaInsets, insets);
    assert.equal(f.frame().props.html, html);
    assert.equal(f.frame().props.key, undefined);
    assert.equal(f.draft(), 'unsaved local edit');
  }
  await f.theme();
  assert.equal(f.app.getHostContext()?.theme, 'dark');
  assert.equal(f.draft(), 'unsaved local edit');
  assert.equal(f.props.item.version, originalVersion);
  assert.equal(f.sessionCount(), 1);
  assert.equal(f.disposals(), 0);
  assert.deepEqual(f.notifications, ['input', 'result']);
  assert.deepEqual(f.writes, []);
  await f.content();
  assert.deepEqual(f.notifications, ['input', 'result', 'result']);
  assert.equal(f.sessionCount(), 1);
});

test('zero-size rotation frames keep last positive geometry and owner retirement still disposes the child', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.layout(852, 393, { top: 48, right: 59, bottom: 21, left: 59 });
  await f.layout(0, 0);
  assert.deepEqual(f.app.getHostContext()?.containerDimensions, { width: 852, height: 393 });
  assert.equal(f.draft(), 'unsaved local edit');
  f.revoke();
  assert.equal(f.disposals(), 1);
  assert.deepEqual(f.writes, []);
});

test('floating close rectangle crosses the actual bridge without changing safe insets or replaying App content', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const html = f.frame().props.html;
  const safe = f.app.getHostContext()?.safeAreaInsets;
  const button = { top: 115, right: 12, width: 44, height: 44 };
  await f.chrome(button);
  assert.deepEqual((f.app.getHostContext()?.zoen as any)?.chrome?.closeButton, button);
  assert.deepEqual(f.app.getHostContext()?.safeAreaInsets, safe);
  await f.chrome({ top: 8, right: 71, width: 44, height: 44 });
  assert.deepEqual((f.app.getHostContext()?.zoen as any)?.chrome?.closeButton, {
    top: 8,
    right: 71,
    width: 44,
    height: 44,
  });
  await f.chrome(null);
  assert.equal((f.app.getHostContext()?.zoen as any)?.chrome?.closeButton, null);
  assert.equal(f.frame().props.html, html);
  assert.equal(f.sessionCount(), 1);
  assert.deepEqual(f.notifications, ['input', 'result']);
  assert.equal(f.draft(), 'unsaved local edit');
  assert.deepEqual(f.writes, []);
});

test('actual App bridge publishes keyboard-sized native layout with its remaining safe area, without replaying content', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const html = f.frame().props.html;
  for (const [height, bottom] of [
    [852, 34],
    [516, 0],
    [652, 0],
    [852, 34],
  ] as const) {
    await f.layout(393, height, { top: 107, right: 0, bottom, left: 0 });
    assert.deepEqual(f.app.getHostContext()?.containerDimensions, { width: 393, height });
    assert.equal(f.app.getHostContext()?.safeAreaInsets?.bottom, bottom);
    assert.equal(f.frame().props.html, html);
    assert.equal(f.draft(), 'unsaved local edit');
  }
  assert.equal(f.sessionCount(), 1);
  assert.equal(f.disposals(), 0);
  assert.deepEqual(f.notifications, ['input', 'result']);
  assert.deepEqual(f.writes, []);
});
