import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { ClientState } from './store.ts';
import type { ContactPreview, LocalMediaFile, MediaDriver } from '../media/types.ts';
import { hooks, nodes } from './attachment-route-ui.fixtures.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
async function compiledPreview(kind: 'contact' | 'location') {
  const path = new URL(`../media/${kind}-attachment.tsx`, import.meta.url);
  const code = transformSync(readFileSync(path, 'utf8'), {
    filename: path.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code;
  const output = await build({
    stdin: {
      contents: code,
      resolveDir: new URL('../media/', import.meta.url).pathname,
      loader: 'js',
    },
    bundle: true,
    write: false,
    format: 'cjs',
    platform: 'node',
    plugins: [
      {
        name: 'contact-native-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) =>
            [
              './use-contact-preview',
              './use-file-preview',
              './contact-preview-session',
              './location-preview-session',
              './native-location.ts',
              './observed-file.ts',
              '../documents/document-authority',
              '../home/access',
              '../home/access.ts',
              './document-authority.ts',
              './access.ts',
            ].includes(path) ||
            path.endsWith('owner-state.ts') ||
            path.startsWith('@babel/runtime/helpers/')
              ? undefined
              : { path, external: true },
          );
        },
      },
    ],
  });

  return output;
}
const contactOutput = await compiledPreview('contact');
const locationOutput = await compiledPreview('location');
export const tick = async () => {
  for (let n = 0; n < 40; n++) await Promise.resolve();
};
export function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
const parsed: ContactPreview = {
  name: 'Ana Real Parser Port',
  phones: [{ label: 'mobile', value: '+55 31 1234 5678' }],
};
const file: LocalMediaFile = {
  uri: 'file:///synthetic/authorized.vcf',
  itemId: 'contact',
  version: 2,
  info: {
    mime: 'text/vcard',
    name: 'contact.vcf',
    path: '',
    sha256: 'a'.repeat(64),
    bytes: '80',
    ready: true,
    chunks: 1,
    chunks_here: 1,
    has_thumbnail: false,
  },
};
export function fixture(kind: 'contact' | 'location' = 'contact') {
  let h = hooks(),
    active = h;
  const wrapper = hooks();
  let key: string | undefined;
  const react = Object.fromEntries(
    Object.keys(h.react).map((name) => [
      name,
      (...args: unknown[]) =>
        (active.react as Record<string, (...args: unknown[]) => unknown>)[name]!(...args),
    ]),
  );
  const state = {
    mode: 'account',
    phase: 'ready',
    erasePending: false,
    snapshot: {
      account: { identity_id: 'owner', device_id: 'phone', unlocked: true, setup_pending: false },
      me: { id: 'owner' },
      conversations: [
        { id: 'space', members: [{ id: 'owner' }] },
        { id: 'other-space', members: [{ id: 'owner' }] },
      ],
    },
  } as unknown as ClientState;
  let nativeRefused = false;
  let spaceId = 'space',
    messageId = 'original-contact-message';
  let scopeCurrent = true,
    parseGate: Promise<ContactPreview> | null = null,
    shareGate: Promise<boolean> | null = null;
  const reads: LocalMediaFile[] = [],
    materialized: unknown[] = [],
    released: string[] = [],
    copies: string[] = [],
    shares: unknown[] = [];
  const location = { longitude: -43.95, latitude: -19.92, label: 'Praça Real' };
  let locationGate: Promise<typeof location> | null = null;
  const actualFile =
    kind === 'contact'
      ? file
      : {
          ...file,
          itemId: 'location',
          uri: 'file:///synthetic/authorized.geojson',
          info: { ...file.info, mime: 'application/geo+json', name: 'location.geojson' },
        };
  const media: MediaDriver = {
    sendFile: async () => {
      throw Error('preview cannot send');
    },
    sendVoice: async () => {
      throw Error('preview cannot record');
    },
    fileInfo: async () => actualFile.info,
    materializeFile: async (itemId, version) => {
      materialized.push({ itemId, version });
      return structuredClone(actualFile);
    },
    readLocation: async (value) => {
      reads.push(value);
      if (nativeRefused) throw Error('native-location-refused');
      return locationGate ?? structuredClone(location);
    },
    readContact: async (value) => {
      reads.push(value);
      if (nativeRefused) throw Error('native-file-authorization-refused');
      return parseGate ?? structuredClone(parsed);
    },
    releaseLocalFile: async (uri) => {
      released.push(uri);
    },
  };
  const listeners = new Set<() => void>(),
    cleanups = new Set<() => Promise<void>>();
  const store = {
    driver: { media },
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    registerSessionCleanup: (cleanup: () => Promise<void>) => {
      cleanups.add(cleanup);
      return () => {
        cleanups.delete(cleanup);
      };
    },
  };
  let focusRead: (() => void | (() => void)) | undefined, focusCleanup: (() => void) | undefined;
  let appListener: ((value: string) => void) | undefined;
  const app = {
    currentState: 'active',
    addEventListener: (_: string, listener: typeof appListener) => {
      appListener = listener;
      return {
        remove: () => {
          appListener = undefined;
        },
      };
    },
  };
  const module = { exports: {} as any };
  const jsx = (type: unknown, props: unknown, key?: string) => ({ type, props, key });
  runInNewContext((kind === 'contact' ? contactOutput : locationOutput).outputFiles[0]!.text, {
    module,
    exports: module.exports,
    Object,
    Promise,
    TextEncoder,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react') return react;
      if (name === 'react/compiler-runtime') return { c: (size: number) => active.runtime.c(size) };
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
      if (name === 'react-native')
        return {
          Text: 'Text',
          View: 'View',
          ScrollView: 'ScrollView',
          ActivityIndicator: 'ActivityIndicator',
          AppState: app,
        };
      if (name === 'expo-router')
        return {
          useFocusEffect: (read: () => void | (() => void)) =>
            active.react.useEffect(() => {
              focusRead = read;
              focusCleanup = read() ?? undefined;
              return focusCleanup;
            }, [read]),
        };
      if (name === '../core/provider') return { useClient: () => ({ store }) };
      if (name === '../components/primitives')
        return { NativeButton: 'NativeButton', IconButton: 'IconButton', Icon: 'Icon' };
      if (name === '../components/message-interaction')
        return { MessagePressable: 'MessagePressable' };
      if (name === './location-map') return { LocationMap: 'LocationMap' };
      if (name === './media-sheet') return { MediaSheet: 'MediaSheet' };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name.startsWith('../i18n'))
        return {
          t: (key: string, values?: Record<string, string>) =>
            values ? key + ':' + Object.values(values).join(',') : key,
        };
      if (name === 'expo-clipboard')
        return {
          setStringAsync: async (value: string) => {
            copies.push(value);
          },
        };
      if (name === 'expo-sharing')
        return {
          isAvailableAsync: async () => shareGate ?? true,
          shareAsync: async (uri: string, options: unknown) => {
            shares.push({ uri, options: structuredClone(options) });
          },
        };
      throw Error(name);
    },
  });
  function once() {
    active = wrapper;
    wrapper.begin();
    const child = module.exports[kind === 'contact' ? 'ContactAttachment' : 'LocationAttachment']({
      media,
      source: { itemId: actualFile.itemId, version: 2, expectedHash: actualFile.info.sha256 },
      spaceId,
      messageId,
      readiness: '80 B',
      message: true,
      isCurrent: () => scopeCurrent,
    });
    wrapper.commit();
    if (key !== undefined && child.key !== key) {
      h.retire();
      h = hooks();
    }
    key = child.key;
    active = h;
    h.begin();
    const tree = child.type(child.props);
    h.commit();
    return tree;
  }
  return {
    state,
    refuse: () => {
      nativeRefused = true;
    },
    context: (space: string, message: string) => {
      spaceId = space;
      messageId = message;
    },
    store,
    reads,
    materialized,
    released,
    copies,
    shares,
    render: () => {
      once();
      return once();
    },
    card: (tree: unknown) => nodes(tree).find((node) => node.type === 'MessagePressable')!,
    sheet: (tree: unknown) => nodes(tree).find((node) => node.type === 'MediaSheet')!,
    button: (tree: unknown, prefix: string) =>
      nodes(tree).find(
        (node) =>
          (node.type === 'NativeButton' || node.type === 'IconButton') &&
          node.props.label.startsWith(prefix),
      )!,
    text: (tree: unknown) =>
      nodes(tree)
        .filter((node) => node.type === 'Text')
        .map((node) => node.props.children),
    locationParse: (gate: Promise<typeof location>) => {
      locationGate = gate;
    },
    parse: (gate: Promise<ContactPreview>) => {
      parseGate = gate;
    },
    share: (gate: Promise<boolean>) => {
      shareGate = gate;
    },
    notify: () => {
      for (const listener of listeners) listener();
    },
    blur: () => focusCleanup?.(),
    focus: () => {
      focusCleanup = focusRead?.() ?? undefined;
    },
    background: () => {
      app.currentState = 'background';
      appListener?.('background');
    },
    foreground: () => {
      app.currentState = 'active';
      appListener?.('active');
    },
    retire: () => {
      h.retire();
      wrapper.retire();
    },
  };
}
