import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
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
function fixture(platform: string) {
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
    'react-native': { View: 'View', Platform: { OS: platform } },
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
    exports: module.exports,
    require: (name: string) => ports[name] ?? require(name),
  });
  function render(html = '<!doctype html><p>Real child document</p>') {
    cursor = 0;
    effects = [];
    const element = module.exports.MiniappFrame(
      {
        html,
        title: 'Drawing',
        height: 640,
        blocked: false,
        onMessage: (message: string) => deliveries.push(message),
        onError: (reason: string) => assert.fail(reason),
      },
      handle,
    );
    const web = element.props.children as Element;
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
    return { receive: web.props.onMessage as (event: unknown) => void, nonce };
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
