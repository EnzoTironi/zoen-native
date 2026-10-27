import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { constants, runInNewContext } from 'node:vm';
import * as React from 'react';
import { App } from '@modelcontextprotocol/ext-apps';
import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import { build } from 'esbuild';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import type { MiniappDocumentController } from '../miniapps/document-controller';

const require = createRequire(import.meta.url),
  { transformSync } = require('@babel/core');
const file = new URL('../miniapps/canonical-view.tsx', import.meta.url);
const compiled = transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  configFile: false,
  babelrc: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;
assert.match(compiled, /react\/compiler-runtime/);
const external = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'expo-router',
  'react-native',
  'react-native-safe-area-context',
  'expo-crypto',
  '../core/provider',
  '../components/primitives',
  '../components/dialogs',
  '../theme',
  '../i18n/index.ts',
  './frame',
  './app-sheet-context',
]);
const built = await build({
  stdin: {
    contents: compiled,
    resolveDir: new URL('../miniapps/', import.meta.url).pathname,
    loader: 'js',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  define: { 'process.env.EXPO_OS': '"ios"' },
  plugins: [
    {
      name: 'compiled-app-reopen-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          path === 'expo-crypto'
            ? { path, namespace: 'native-crypto-port' }
            : external.has(path) || path === 'app-reopen-ports'
              ? { path, external: true }
              : undefined,
        );
        builder.onLoad({ filter: /.*/, namespace: 'native-crypto-port' }, () => ({
          contents: `const p=require('app-reopen-ports')['expo-crypto']; export const randomUUID=p.randomUUID; export const digestStringAsync=p.digestStringAsync; export const CryptoDigestAlgorithm=p.CryptoDigestAlgorithm; export const CryptoEncoding=p.CryptoEncoding;`,
        }));
        builder.onLoad({ filter: /\/miniapps\/protocol\.ts$/ }, ({ path }) => ({
          contents: readFileSync(process.env.ZOEN_APP_REOPEN_PROTOCOL_SOURCE ?? path, 'utf8'),
          loader: 'ts',
        }));
      },
    },
  ],
});

type Props = Record<string, any>;
const hash = (text: string) => createHash('sha256').update(text).digest('hex');
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));

/** Compiled production drawing lifetime, loader, scene host, persistence and
 * official SDK. Core and the ordered native frame transport are explicit ports. */
export async function appReopenFixture() {
  const root = createReactTestRoot(),
    focused = React.createContext(true);
  const html = '<html><head></head><body>Owned MCP App</body></html>';
  let scene = '{"elements":[],"appState":{},"files":{}}',
    version = 3;
  const uri = `ui://zoen/package/release/1/${'ab'.repeat(32)}`;
  let currentUri = uri,
    epoch = 1,
    device = 'device',
    controller: MiniappDocumentController | null = null;
  const commands: Props[] = [],
    wires: ReturnType<typeof wire>[] = [],
    texts: unknown[] = [],
    listeners = new Set<() => void>();
  const item = {
    id: 'same-item',
    space_id: 'space',
    title: 'Drawing',
    version,
    app: { app_id: 'excalidraw', resource_uri: uri },
  };
  const doc = () => ({
    item_id: item.id,
    version,
    current_version: version,
    scene_json: scene,
    sha256: hash(scene),
  });
  const snapshot = () => ({
    mode: 'account',
    phase: 'ready',
    erasePending: false,
    snapshot: {
      account: { identity_id: 'owner', device_id: device, unlocked: true, setup_pending: false },
      conversations: [{ id: 'space' }],
    },
  });
  const checkPin = (request: Props) => {
    assert.equal(request.expected_resource_uri, uri);
    if (request.expected_resource_uri !== currentUri) throw Error('The App release changed.');
  };
  const store = {
    get sessionEpoch() {
      return epoch;
    },
    getSnapshot: snapshot,
    subscribe: (callback: () => void) => {
      listeners.add(callback);
      return () => listeners.delete(callback);
    },
    subscribeCanonicalChanges: () => () => {},
    registerSessionPreflight: () => () => {},
    registerSessionCleanup: () => () => {},
    driver: {
      request: async (request: Props) => {
        commands.push(request);
        checkPin(request);
        if (request.op === 'app_document_read_pinned') return doc();
        if (request.op === 'app_document_resource')
          return {
            uri,
            mime_type: 'text/html;profile=mcp-app',
            text: html,
            prefers_border: false,
            manifest_json: JSON.stringify({
              id: 'excalidraw',
              name: 'Drawing',
              allowedDomains: [],
              capabilities: [],
              sha256: hash(html),
              bytes: Buffer.byteLength(html),
            }),
          };
        throw Error(`Unexpected read ${request.op}`);
      },
    },
    request: async (request: Props) => {
      commands.push(request);
      checkPin(request);
      assert.equal(request.op, 'app_document_save_pinned');
      assert.equal(request.expected_version, version);
      assert.equal(request.expected_sha256, hash(scene));
      scene = request.scene_json;
      version++;
      return doc();
    },
  };
  function wire(receive: (message: string) => void) {
    const app = new App(
      { name: 'Owned drawing', version: '1' },
      { tools: {} },
      { autoResize: false },
    );
    const queue: JSONRPCMessage[] = [];
    let live = true,
      loaded: string | null = null,
      error: unknown = null,
      draft: string | null = null;
    const transport: Transport = {
      start: async () => {},
      close: async () => {
        transport.onclose?.();
      },
      send: async (message) => {
        if (live) queue.push(message);
      },
    };
    const call = async (name: string, args: Props) => {
      const result = await app.callServerTool({ name, arguments: args });
      if (result.isError) throw Error('The scene was refused.');
      return result.structuredContent as Props;
    };
    const checkpoint = async () => {
      if (!draft) return;
      const pending = draft,
        bytes = Buffer.from(pending);
      const begin = await call('app_scene_begin', {
        id: item.id,
        mutationId: randomUUID(),
        bytes: bytes.length,
        sha256: hash(pending),
      });
      await call('app_scene_append', {
        id: item.id,
        uploadId: begin.uploadId,
        index: 0,
        data: bytes.toString('base64'),
      });
      await call('app_scene_commit', { id: item.id, uploadId: begin.uploadId });
      draft = null;
      loaded = pending;
    };
    void app
      .connect(transport)
      .then(async () => {
        const meta = await call('app_scene_read', { id: item.id });
        const chunk = await call('app_scene_read_chunk', {
          id: item.id,
          transferId: meta.transferId,
          index: 0,
        });
        loaded = Buffer.from(chunk.data, 'base64').toString('utf8');
        assert.equal(hash(loaded), meta.sha256);
        app.onteardown = async () => {
          await checkpoint();
          return {};
        };
      })
      .catch((reason: unknown) => {
        error = reason;
      });
    return {
      app,
      queue,
      loaded: () => loaded,
      error: () => error,
      draft: () => draft,
      edit: (text: string) => {
        draft = text;
      },
      send: (message: unknown) => {
        if (live) transport.onmessage?.(message as JSONRPCMessage);
      },
      deliver: () => {
        for (const message of queue.splice(0)) receive(JSON.stringify(message));
      },
      dispose: () => {
        live = false;
        void app.close();
      },
    };
  }
  const Frame = React.forwardRef(function Frame(p: Props, ref) {
    const latest = React.useRef(p);
    React.useLayoutEffect(() => {
      latest.current = p;
    }, [p]);
    const current = React.useRef<ReturnType<typeof wire> | null>(null);
    React.useImperativeHandle(
      ref,
      () => ({
        send: (message: unknown) => current.current?.send(message),
        dispose: () => current.current?.dispose(),
      }),
      [],
    );
    React.useLayoutEffect(() => {
      const native = wire((message) => latest.current.onMessage(message));
      current.current = native;
      wires.push(native);
      return () => {
        if (current.current === native) current.current = null;
        native.dispose();
      };
    }, []);
    return null;
  });
  const theme = { colors: { background: '#000', text: '#fff' }, scheme: 'dark' },
    dialogs = {};
  const children = (p: Props) => p.children;
  const ports: Props = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'expo-router': {
      router: { back() {} },
      useFocusEffect: (effect: () => void | (() => void)) => {
        const active = React.useContext(focused);
        React.useEffect(() => (active ? effect() : undefined), [active, effect]);
      },
    },
    'react-native': {
      View: children,
      Text: (p: Props) => {
        React.useLayoutEffect(() => {
          texts.push(p.children);
        }, [p.children]);
        return null;
      },
      Linking: {},
      AppState: { addEventListener: () => ({ remove() {} }) },
    },
    'react-native-safe-area-context': {
      useSafeAreaInsets: () => ({ top: 0, right: 0, bottom: 0, left: 0 }),
    },
    'expo-crypto': {
      randomUUID,
      CryptoDigestAlgorithm: { SHA256: 'SHA-256' },
      CryptoEncoding: { HEX: 'hex' },
      digestStringAsync: async (_kind: string, text: string) => hash(text),
    },
    '../core/provider': { useClient: () => ({ store }) },
    '../components/primitives': {
      Loading: () => null,
      NativeButton: () => null,
      Surface: children,
    },
    '../components/dialogs': { useDialogs: () => dialogs },
    '../theme': { useTheme: () => theme },
    '../i18n/index.ts': { locale: 'en', t: (key: string) => key },
    './frame': { MiniappFrame: Frame },
    './app-sheet-context': { useAppSheetCloseButton: () => null },
  };
  const module = { exports: {} as { CanonicalDrawingView: React.ComponentType<Props> } };
  runInNewContext(
    built.outputFiles[0]!.text,
    {
      module,
      exports: module.exports,
      global: { Error },
      setTimeout,
      clearTimeout,
      setInterval,
      clearInterval,
      TextEncoder,
      TextDecoder,
      URL,
      AbortController,
      console,
      require: (name: string) =>
        name === 'app-reopen-ports' ? ports : (ports[name] ?? require(name)),
    },
    { importModuleDynamically: constants.USE_MAIN_CONTEXT_DEFAULT_LOADER },
  );
  const spec = { id: 'excalidraw', resource_uri: 'ui://zoen/excalidraw.html' };
  const register = (next: MiniappDocumentController | null) => {
    controller = next;
  };
  const render = (active = true) =>
    root.render(
      React.createElement(
        focused.Provider,
        { value: active },
        React.createElement(module.exports.CanonicalDrawingView, {
          item,
          spec,
          onDocumentController: register,
        }),
      ),
    );
  return {
    commands,
    wires,
    texts,
    render,
    act: root.act,
    errors: root.errors,
    controller: () => {
      assert.ok(controller);
      return controller;
    },
    scene: () => scene,
    async pump() {
      await root.act(async () => {
        for (let round = 0; round < 12; round++) {
          await tick();
          const queued = wires.some((entry) => entry.queue.length);
          for (const entry of wires) entry.deliver();
          if (!queued && round > 2) break;
        }
      });
    },
    changePin: () => {
      currentUri = `ui://zoen/package/release/2/${'cd'.repeat(32)}`;
    },
    retireOwner: () => {
      epoch++;
      device = 'new-device';
      for (const listener of listeners) listener();
    },
    close: root.close,
  };
}
