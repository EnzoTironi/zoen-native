import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
import type { RuntimePackage } from './runtime-package';

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
  const focused: number[] = [];
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
    process: { env: { EXPO_PUBLIC_ZOEN_DECKCRAFT_INPUT_QA: qa ? '1' : undefined } },
    exports: module.exports,
    require: (name: string) => ports[name] ?? require(name),
  });
  function render(
    html = '<!doctype html><p>Real child document</p>',
    geometry: { height: number; fill: boolean; blocked?: boolean } = {
      height: 640,
      fill: false,
    },
    runtimePackage?: RuntimePackage,
  ) {
    cursor = 0;
    effects = [];
    const element = module.exports.MiniappFrame(
      {
        html,
        title: 'Drawing',
        ...geometry,
        blocked: geometry.blocked ?? false,
        runtimePackage,
        onMessage: (message: string) => deliveries.push(message),
        onError: (reason: string) => assert.fail(reason),
      },
      handle,
    );
    const children = element.props.children as (Element | null)[];
    const web = children[0]!;
    assert.equal(web.type, 'WebView');
    (web.props.ref as { current: unknown }).current = {
      injectJavaScript: (code: string) => injected.push(code),
      stopLoading() {},
      requestFocus: () => focused.push(1),
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
      qaScript: web.props.injectedJavaScriptBeforeContentLoaded as string | undefined,
      qaMainOnly: web.props.injectedJavaScriptBeforeContentLoadedForMainFrameOnly,
      qaLabel: children[1]?.props.accessibilityLabel as string | undefined,
      key: web.props.key,
      touch: web.props.onTouchStart as () => void,
      fail: web.props.onError as (event: unknown) => void,
      terminate: web.props.onContentProcessDidTerminate as () => void,
    };
  }
  return { render, deliveries, handle, injected, focused };
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

test('iOS native focus follows a real touch only, with no document reload on geometry changes', () => {
  const f = fixture('ios');
  const portrait = f.render();
  assert.equal(f.focused.length, 0);
  portrait.touch();
  assert.equal(f.focused.length, 1);
  const rotated = f.render(undefined, { height: 360, fill: true });
  assert.equal(rotated.source, portrait.source);
  assert.equal(rotated.nonce, portrait.nonce);
  assert.equal(f.focused.length, 1);
  rotated.touch();
  assert.equal(f.focused.length, 2);
});

test('native input focus refuses blocked, retired and non-iOS touches, including stale callbacks', () => {
  const f = fixture('ios');
  const original = f.render();
  const blocked = f.render(undefined, { height: 640, fill: false, blocked: true });
  original.touch();
  blocked.touch();
  assert.equal(f.focused.length, 0);
  const resumed = f.render();
  resumed.touch();
  assert.equal(f.focused.length, 1);
  const next = f.render('<!doctype html><p>New owner</p>');
  resumed.touch();
  assert.equal(f.focused.length, 1);
  f.handle.current!.dispose();
  next.touch();
  assert.equal(f.focused.length, 1);
  for (const platform of ['android', 'web', 'macos']) {
    const other = fixture(platform);
    other.render().touch();
    assert.equal(other.focused.length, 0, platform);
  }
});

test('retired native callbacks cannot poison a replacement App with fatal frame feedback', () => {
  const f = fixture('ios');
  const old = f.render();
  const current = f.render('<!doctype html><p>Replacement</p>');
  assert.doesNotThrow(() => old.fail({ nativeEvent: { description: 'retired failure' } }));
  assert.doesNotThrow(() => old.terminate());
  assert.throws(
    () => current.fail({ nativeEvent: { description: 'current failure' } }),
    /current failure/,
  );
  assert.throws(() => current.terminate(), /theSystemClosedThisInterfaceRefreshToOpen/);
  f.handle.current!.dispose();
  assert.doesNotThrow(() => current.fail({ nativeEvent: { description: 'disposed failure' } }));
  assert.doesNotThrow(() => current.terminate());
});

const inputQaRevision = 'a31b990d32f2c8562f1588562f922960c3fde48b';
const inputQaRuntime: RuntimePackage = {
  schema: 1,
  engine: 'deckcraft',
  revision: inputQaRevision,
  abi: 'zoen-host-v1',
  module: {
    url: `https://127.0.0.1:9443/deckcraft/${inputQaRevision}/runtime.js`,
    bytes: 1,
    sha256: '0'.repeat(64),
    mime: 'text/javascript',
  },
  wasm: {
    url: `https://127.0.0.1:9443/deckcraft/${inputQaRevision}/runtime.wasm`,
    bytes: 8,
    sha256: '1'.repeat(64),
    mime: 'application/wasm',
  },
};
function inputQaSession(script: string | undefined) {
  const token = script?.match(/const session="([a-f0-9-]+)"/)?.[1];
  assert.ok(token);
  return token;
}
function inputQaEvent(session: string, extra: Record<string, unknown> = {}) {
  return {
    nativeEvent: {
      url: 'about:srcdoc',
      data: JSON.stringify({
        marker: 'zoen.deckcraft.input-order.qa.v1',
        session,
        rows: [[1, 1, 0, 1, 1, 0, 14, 14, 14, 1, 0, 0, 1]],
        truncated: false,
        ...extra,
      }),
    },
  };
}

test('input-order observation is default-off and absent outside the configured iOS DeckCraft', () => {
  for (const platform of ['ios', 'android', 'web', 'macos']) {
    const disabled = fixture(platform).render(undefined, undefined, inputQaRuntime);
    assert.equal(disabled.qaScript, undefined, platform);
    assert.equal(disabled.qaMainOnly, undefined, platform);
    assert.equal(disabled.qaLabel, undefined, platform);
  }
  for (const platform of ['android', 'web', 'macos']) {
    const other = fixture(platform, true).render(undefined, undefined, inputQaRuntime);
    assert.equal(other.qaScript, undefined, platform);
    assert.equal(other.qaLabel, undefined, platform);
  }
  const generic = fixture('ios', true).render();
  assert.equal(generic.qaScript, undefined);
  assert.equal(generic.qaLabel, undefined);
  const wrongRevision = fixture('ios', true).render(undefined, undefined, {
    ...inputQaRuntime,
    revision: '0'.repeat(40),
    module: {
      ...inputQaRuntime.module,
      url: inputQaRuntime.module.url.replace(inputQaRevision, '0'.repeat(40)),
    },
    wasm: {
      ...inputQaRuntime.wasm,
      url: inputQaRuntime.wasm.url.replace(inputQaRevision, '0'.repeat(40)),
    },
  });
  assert.equal(wrongRevision.qaScript, undefined);
  assert.equal(wrongRevision.qaLabel, undefined);
});

test('actual native frame consumes only bounded input scalars without canonical delivery or reload', () => {
  const f = fixture('ios', true);
  const doc = f.render(undefined, undefined, inputQaRuntime);
  const session = inputQaSession(doc.qaScript);
  assert.notEqual(session, doc.nonce);
  assert.equal(doc.qaMainOnly, false, 'observe the opaque canvas child at document start');
  assert.equal(doc.qaLabel, 'input trace: pending');
  doc.receive(inputQaEvent(session));
  assert.deepEqual(f.deliveries, [], 'numeric diagnostics never reach SDK/core RPC');
  const updated = f.render(undefined, { height: 393, fill: true }, inputQaRuntime);
  assert.equal(updated.source, doc.source);
  assert.equal(updated.nonce, doc.nonce);
  assert.equal(updated.qaScript, doc.qaScript);
  assert.equal(updated.qaLabel, 'input trace: 1 rows; truncated=0; 1/1/0/1/1/0/14/14/14/1/0/0/1');
  assert.equal(
    f.injected.some((code) => code.includes('"dispose"')),
    false,
  );
  doc.receive(event('about:blank', doc.nonce));
  assert.equal(f.deliveries.length, 1, 'ordinary canonical source/nonce admission is unchanged');
  doc.receive(event('about:srcdoc', doc.nonce));
  assert.equal(f.deliveries.length, 1, 'diagnostics never loosen opaque-child RPC source policy');
});

test('input-order frame refuses stale tokens, extra fields and wrong scalar types', () => {
  const f = fixture('ios', true);
  const doc = f.render(undefined, undefined, inputQaRuntime);
  const token = inputQaSession(doc.qaScript);
  for (const bad of [
    inputQaEvent(doc.nonce),
    inputQaEvent('00000000-0000-4000-8000-999999999999'),
    inputQaEvent(token, { text: 'private content must not be displayed' }),
    inputQaEvent(token, { rows: [[1, 1, 0, 1, 1, 0, 'private', 0, 0, 1, 0, 0, 1]] }),
  ])
    doc.receive(bad);
  const unchanged = f.render(undefined, undefined, inputQaRuntime);
  assert.equal(unchanged.qaLabel, 'input trace: pending');
  assert.equal(unchanged.source, doc.source);
  assert.deepEqual(f.deliveries, []);
});

test('replacement and explicit disposal retire diagnostic state and old callback authority', () => {
  const f = fixture('ios', true);
  const old = f.render(undefined, undefined, inputQaRuntime);
  old.receive(inputQaEvent(inputQaSession(old.qaScript)));
  const currentHtml = '<!doctype html><p>Replacement verified resource</p>';
  const current = f.render(currentHtml, undefined, inputQaRuntime);
  assert.notEqual(current.source, old.source);
  assert.equal(current.qaLabel, 'input trace: pending');
  old.receive(inputQaEvent(inputQaSession(old.qaScript)));
  current.receive(inputQaEvent(inputQaSession(old.qaScript)));
  assert.equal(f.render(currentHtml, undefined, inputQaRuntime).qaLabel, 'input trace: pending');
  current.receive(inputQaEvent(inputQaSession(current.qaScript)));
  assert.match(f.render(currentHtml, undefined, inputQaRuntime).qaLabel!, /^input trace: 1 rows;/);
  f.handle.current!.dispose();
  current.receive(inputQaEvent(inputQaSession(current.qaScript)));
  const disposed = f.render(currentHtml, undefined, inputQaRuntime);
  assert.equal(disposed.source, current.source);
  assert.equal(disposed.qaLabel, 'input trace: pending');
  assert.deepEqual(f.deliveries, []);
});
