import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import type { RuntimePackage } from './runtime-package';
import { build } from 'esbuild';

const source = new URL('./frame.tsx', import.meta.url);
const compiled = await build({
  entryPoints: [fileURLToPath(source)],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'native-frame-ports',
      setup(builder) {
        builder.onResolve(
          {
            filter:
              /^(react(?:\/jsx-runtime)?|react-native(?:-webview)?|expo-crypto|\.\.\/i18n\/index\.ts)$/,
          },
          ({ path }) => ({ path, external: true }),
        );
      },
    },
  ],
});
const require = createRequire(import.meta.url);
type Element = { type: unknown; props: Record<string, unknown> };
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
function fixture(platform: string, qa = false) {
  const slots: Slot[] = [];
  let cursor = 0;
  let sequence = 0;
  let effects: (() => void)[] = [];
  const deliveries: string[] = [];
  const injected: string[] = [];
  const handle: { current: { dispose(): void } | null } = { current: null };
  const slot = () => (slots[cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    !!a && !!b && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const ports: Record<string, unknown> = {
    react: {
      forwardRef: (render: unknown) => render,
      useRef: (initial: unknown) => {
        const s = slot();
        if (!Object.hasOwn(s, 'value')) s.value = { current: initial };
        return s.value;
      },
      useState: (initial: unknown) => {
        const s = slot();
        if (!Object.hasOwn(s, 'value'))
          s.value = typeof initial === 'function' ? (initial as () => unknown)() : initial;
        return [
          s.value,
          (next: unknown) => {
            s.value =
              typeof next === 'function' ? (next as (prior: unknown) => unknown)(s.value) : next;
          },
        ];
      },
      useMemo: (read: () => unknown, deps: readonly unknown[]) => {
        const s = slot();
        if (!same(s.deps, deps)) {
          s.deps = deps;
          s.value = read();
        }
        return s.value;
      },
      useLayoutEffect: (read: () => (() => void) | void, deps: readonly unknown[]) => {
        const s = slot();
        if (!same(s.deps, deps)) {
          s.deps = deps;
          effects.push(() => {
            s.cleanup?.();
            s.cleanup = read() ?? undefined;
          });
        }
      },
      useImperativeHandle: (ref: typeof handle, read: () => typeof handle.current) => {
        ref.current = read();
      },
    },
    'react/jsx-runtime': {
      jsx: (type: unknown, props: Record<string, unknown>) => ({ type, props }),
      jsxs: (type: unknown, props: Record<string, unknown>) => ({ type, props }),
    },
    'react-native': { Text: 'Text', View: 'View', Platform: { OS: platform } },
    'react-native-webview': { WebView: 'WebView' },
    'expo-crypto': {
      randomUUID: () => `00000000-0000-4000-8000-${String(++sequence).padStart(12, '0')}`,
    },
    '../i18n/index.ts': { t: (key: string) => key },
  };
  const module = {
    exports: {} as {
      MiniappFrame: (props: Record<string, unknown>, ref: typeof handle) => Element;
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    TextEncoder,
    TextDecoder,
    URL,
    process: { env: { EXPO_PUBLIC_ZOEN_DECKCRAFT_FOCUS_QA: qa ? '1' : undefined } },
    exports: module.exports,
    require: (name: string) => ports[name] ?? require(name),
  });
  function render(
    html = '<!doctype html><p>Real child document</p>',
    geometry = { height: 640, fill: false },
    runtimePackage?: RuntimePackage,
  ) {
    cursor = 0;
    effects = [];
    const element = module.exports.MiniappFrame(
      {
        html,
        title: 'Drawing',
        ...geometry,
        blocked: false,
        runtimePackage,
        onMessage: (message: string) => deliveries.push(message),
        onError: (reason: string) => assert.fail(reason),
      },
      handle,
    );
    const children = element.props.children as Element[];
    const web = children[0]!;
    assert.equal(web.type, 'WebView');
    (web.props.ref as { current: unknown }).current = {
      injectJavaScript: (code: string) => injected.push(code),
      stopLoading() {},
    };
    for (const commit of effects) commit();
    const nonce = (web.props.source as { html: string }).html.match(
      /const nonce="([a-f0-9-]+)"/,
    )?.[1];
    assert.ok(nonce);
    return {
      receive: web.props.onMessage as (event: unknown) => void,
      nonce,
      source: web.props.source,
      key: web.props.key,
      qaText: children[1] as Element | null,
      qaScript: web.props.injectedJavaScriptBeforeContentLoaded as string | undefined,
      qaMainFrameOnly: web.props.injectedJavaScriptBeforeContentLoadedForMainFrameOnly,
    };
  }
  return { render, deliveries, handle, injected };
}
const rpc = { jsonrpc: '2.0', id: 1, method: 'ui/initialize', params: {} };
function event(url: string, nonce: string, message: unknown = rpc) {
  return {
    nativeEvent: { url, data: JSON.stringify({ nonce, message: JSON.stringify(message) }) },
  };
}

test('actual native callback accepts modern Android opaque-source envelopes and both existing shell URLs', () => {
  const f = fixture('android');
  const doc = f.render();
  for (const url of ['null', 'about:blank', '']) doc.receive(event(url, doc.nonce));
  assert.deepEqual(
    f.deliveries,
    [rpc, rpc, rpc].map((value) => JSON.stringify(value)),
  );
});

test('opaque source is Android-only and never a URL-prefix or arbitrary-origin allowance', () => {
  for (const platform of ['ios', 'web', 'macos']) {
    const f = fixture(platform);
    const doc = f.render();
    doc.receive(event('null', doc.nonce));
    assert.equal(f.deliveries.length, 0, platform);
    doc.receive(event('about:blank', doc.nonce));
    assert.equal(f.deliveries.length, 1, platform);
  }
  const f = fixture('android');
  const doc = f.render();
  for (const url of [
    'null#child',
    'NULL',
    'about:blank#child',
    'about:srcdoc',
    'https://example.com',
    'file:///private',
    'content://provider/file',
    'data:text/html,child',
  ])
    doc.receive(event(url, doc.nonce));
  assert.deepEqual(f.deliveries, []);
});

test('modern origin never bypasses the exact document nonce, envelope shape or JSON-RPC validation', () => {
  const f = fixture('android');
  const doc = f.render();
  doc.receive(event('null', '00000000-0000-4000-8000-999999999999'));
  doc.receive(event('null', doc.nonce, { not: 'JSON RPC' }));
  doc.receive({ nativeEvent: { url: 'null', data: JSON.stringify(rpc) } });
  doc.receive({
    nativeEvent: {
      url: 'null',
      data: JSON.stringify({ nonce: doc.nonce, message: JSON.stringify(rpc), extra: true }),
    },
  });
  assert.deepEqual(f.deliveries, []);
  doc.receive(event('null', doc.nonce));
  assert.equal(f.deliveries.length, 1);
});

test('an old native callback and a disposed frame cannot deliver into the replacement document', () => {
  const f = fixture('android');
  const old = f.render();
  const next = f.render('<!doctype html><p>Replacement</p>');
  assert.notEqual(old.nonce, next.nonce);
  old.receive(event('null', old.nonce));
  next.receive(event('null', old.nonce));
  assert.deepEqual(f.deliveries, []);
  next.receive(event('null', next.nonce));
  assert.equal(f.deliveries.length, 1);
  f.handle.current!.dispose();
  next.receive(event('null', next.nonce));
  assert.equal(f.deliveries.length, 1);
});

test('portrait-landscape-portrait layout keeps one native source, nonce and callback authority', () => {
  const f = fixture('ios');
  const html = '<!doctype html><p>Unsaved editor state belongs to this document</p>';
  const portrait = f.render(html, { height: 852, fill: true });
  const landscape = f.render(html, { height: 393, fill: true });
  const returned = f.render(html, { height: 852, fill: true });
  assert.equal(landscape.source, portrait.source);
  assert.equal(returned.source, portrait.source);
  assert.equal(landscape.nonce, portrait.nonce);
  assert.equal(returned.nonce, portrait.nonce);
  assert.equal(returned.key, undefined);
  assert.equal(
    f.injected.some((code) => code.includes('"dispose"')),
    false,
  );
  portrait.receive(event('about:blank', portrait.nonce));
  landscape.receive(event('about:blank', landscape.nonce));
  returned.receive(event('about:blank', returned.nonce));
  assert.equal(f.deliveries.length, 3);
  const replaced = f.render('<!doctype html><p>New verified resource/policy scope</p>');
  assert.notEqual(replaced.source, portrait.source);
  assert.notEqual(replaced.nonce, portrait.nonce);
  returned.receive(event('about:blank', returned.nonce));
  assert.equal(f.deliveries.length, 3);
});

const qaRevision = 'a31b990d32f2c8562f1588562f922960c3fde48b';
const qaRuntime: RuntimePackage = {
  schema: 1,
  engine: 'deckcraft',
  revision: qaRevision,
  abi: 'zoen-host-v1',
  module: {
    url: `https://127.0.0.1:9443/deckcraft/${qaRevision}/runtime.js`,
    bytes: 1,
    sha256: '0'.repeat(64),
    mime: 'text/javascript',
  },
  wasm: {
    url: `https://127.0.0.1:9443/deckcraft/${qaRevision}/runtime.wasm`,
    bytes: 8,
    sha256: '1'.repeat(64),
    mime: 'application/wasm',
  },
};
function qaEvent(session: string, slot: 'top' | 'childCanvas' = 'childCanvas') {
  return {
    nativeEvent: {
      url: slot === 'top' ? 'about:blank' : 'about:srcdoc',
      data: JSON.stringify({
        marker: 'zoen.deckcraft.focus.qa.v1',
        session,
        slot,
        event: 'focus',
        activeTag: 'INPUT',
        inputCount: 1,
        hasFocus: true,
        canvasFocused: false,
        inputFocused: true,
        isTop: slot === 'top',
        inert: false,
        disabled: false,
      }),
    },
  };
}
function qaSession(script: string | undefined): string {
  const token = script?.match(/const session="([a-f0-9-]+)"/)?.[1];
  assert.ok(token);
  return token;
}

test('temporary focus QA is absent by default and never enabled on other platform/resource scopes', () => {
  const disabled = fixture('ios').render(undefined, undefined, qaRuntime);
  assert.equal(disabled.qaScript, undefined);
  assert.equal(disabled.qaMainFrameOnly, undefined);
  assert.equal(disabled.qaText, null);
  const generic = fixture('ios', true).render();
  assert.equal(generic.qaScript, undefined);
  const android = fixture('android', true).render(undefined, undefined, qaRuntime);
  assert.equal(android.qaScript, undefined);
});

test('actual native QA callback updates two scalar AX slots without forwarding telemetry to SDK', () => {
  const f = fixture('ios', true);
  const first = f.render(undefined, undefined, qaRuntime);
  const token = qaSession(first.qaScript);
  assert.notEqual(token, first.nonce, 'the child must never receive the canonical RPC nonce');
  assert.equal(first.qaMainFrameOnly, false);
  first.receive(qaEvent(token));
  first.receive(qaEvent(token, 'top'));
  const next = f.render(undefined, undefined, qaRuntime);
  assert.equal(next.source, first.source, 'diagnostic state cannot reload the App');
  assert.equal(next.qaText?.props.testID, 'deckcraft-focus-qa');
  const label = String(next.qaText?.props.accessibilityLabel);
  assert.match(label, /top: focus/);
  assert.match(label, /childCanvas: focus tag=INPUT/);
  assert.equal(label.includes(token), false);
  assert.equal(label.includes(first.nonce), false);
  assert.deepEqual(f.deliveries, []);
  next.receive(event('about:blank', next.nonce));
  assert.deepEqual(f.deliveries, [JSON.stringify(rpc)]);
});

test('QA rejects stale generation, extra fields and post-dispose callbacks without affecting RPC', () => {
  const f = fixture('ios', true);
  const old = f.render(undefined, undefined, qaRuntime);
  const token = qaSession(old.qaScript);
  const next = f.render('<!doctype html><p>Replacement</p>', undefined, qaRuntime);
  old.receive(qaEvent(token));
  next.receive(qaEvent(token));
  const invalid = qaEvent(qaSession(next.qaScript));
  invalid.nativeEvent.data = JSON.stringify({
    ...JSON.parse(invalid.nativeEvent.data),
    value: 'rejected',
  });
  next.receive(invalid);
  const settled = f.render('<!doctype html><p>Replacement</p>', undefined, qaRuntime);
  assert.equal(settled.qaText?.props.accessibilityLabel, 'top: pending | childCanvas: pending');
  assert.deepEqual(f.deliveries, []);
  f.handle.current!.dispose();
  settled.receive(qaEvent(qaSession(settled.qaScript)));
  assert.deepEqual(f.deliveries, []);
});
