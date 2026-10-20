import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
const path = new URL('./deckcraft-view.tsx', import.meta.url);
const compiled = await build({
  stdin: {
    contents: readFileSync(path, 'utf8') + '\nexport {DeckcraftDocument};',
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
      name: 'owned-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /.*/ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
type Element = { type: unknown; props: Record<string, any> };
function find(tree: any, id: string): Element | undefined {
  if (Array.isArray(tree)) return tree.flatMap((v) => find(v, id) ?? [])[0];
  if (!tree?.props) return;
  return tree.props.testID === id ? tree : find(tree.props.children, id);
}
function fixture() {
  const slots: any[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  let retries = 0;
  let owner: any;
  let controller: any;
  const same = (a: any[], b: any[]) =>
    a && b && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const slot = () => slots[cursor++] ?? (slots[cursor - 1] = {});
  const memo = (read: () => unknown, deps: any[]) => {
    const s = slot();
    if (!same(s.deps, deps)) {
      s.deps = deps;
      s.value = read();
    }
    return s.value;
  };
  const effect = (read: () => unknown, deps: any[]) => {
    const s = slot();
    if (!same(s.deps, deps)) {
      s.deps = deps;
      effects.push(() => {
        s.cleanup?.();
        s.cleanup = read();
      });
    }
  };
  const store = {
    driver: {},
    request: async () => {},
    registerSessionPreflight: () => () => {},
    registerSessionCleanup: () => () => {},
    subscribe: () => () => {},
  };
  const insets = { top: 62, right: 0, bottom: 34, left: 0 };
  const theme = { colors: { background: '#000', text: '#fff' }, scheme: 'dark' };
  const ports: Record<string, unknown> = {
    react: {
      useState: (initial: unknown) => {
        const s = slot();
        if (!('value' in s)) s.value = initial;
        return [
          s.value,
          (next: any) => {
            s.value = typeof next === 'function' ? next(s.value) : next;
          },
        ];
      },
      useRef: (initial: unknown) => {
        const s = slot();
        return s.value ?? (s.value = { current: initial });
      },
      useMemo: memo,
      useCallback: (fn: any, deps: any[]) => memo(() => fn, deps),
      useLayoutEffect: effect,
      useEffect: effect,
    },
    'react/jsx-runtime': {
      jsx: (type: unknown, props: any) => ({ type, props }),
      jsxs: (type: unknown, props: any) => ({ type, props }),
      Fragment: 'Fragment',
    },
    'react-native': {
      Text: 'Text',
      View: 'View',
      AppState: { addEventListener: () => ({ remove: () => {} }) },
    },
    'expo-router': { router: { back: () => {} } },
    'expo-crypto': { randomUUID: () => 'owned' },
    'react-native-safe-area-context': { useSafeAreaInsets: () => insets },
    '../i18n/index.ts': { t: (key: string) => key, locale: 'en' },
    '../core/provider': { useClient: () => ({ store }) },
    '../components/primitives': {
      Loading: 'Loading',
      NativeButton: 'NativeButton',
      Surface: 'Surface',
    },
    '../theme': { useTheme: () => theme },
    './app-sheet-context': { useAppSheetCloseButton: () => null },
    './canonical-binary': {},
    './deckcraft-export-files': {},
    './frame': { MiniappFrame: 'MiniappFrame' },
    './resource-load': {},
    './sandbox': { sandboxHtml: () => '<p>Owned</p>' },
    './lifetime': { frameAuthority: (check: () => void) => ({ check, close: () => {} }) },
    './deckcraft-session': {
      createDeckcraftSession: (options: any) => {
        const current = {
          opened: false,
          retired: false,
          controller: { flush: async () => {} },
          receive: () => {},
          syncContext: () => {},
          drain: async () => {},
          retire: () => {
            current.retired = true;
          },
          state: options.onState,
        };
        owner = current;
        return current;
      },
    },
  };
  const props = {
    item: { id: 'owned', title: 'Owned' },
    fill: true,
    verified: { resource: { text: '<p>Owned</p>' }, manifest: {} },
    checkOwner: () => {},
    registerClose: () => () => {},
    onInitialRetry: () => {
      retries++;
    },
    onDocumentController: (value: any) => {
      controller = value;
    },
  };
  const module = { exports: {} as any };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
    Intl,
  });
  const render = () => {
    cursor = 0;
    effects = [];
    const tree = module.exports.DeckcraftDocument(props);
    for (const commit of effects) commit();
    return tree;
  };
  render();
  return {
    render,
    owner: () => owner,
    error: () =>
      owner.state({
        phase: 'error',
        opened: false,
        blocked: true,
        error: 'Synthetic initial open failure',
      }),
    opened: () => {
      owner.opened = true;
      owner.state({ phase: 'saved', opened: true, blocked: false, error: null });
    },
    retries: () => retries,
    close: () => {
      for (const s of slots) s.cleanup?.();
    },
    controller: () => controller,
  };
}
test('compiled initial failure offers fresh-open retry, while stale retry cannot remount an opened or retired document', () => {
  const f = fixture();
  assert.ok(f.controller());
  f.error();
  const button = find(f.render(), 'deckcraft-retry-open')!;
  assert.ok(button);
  button.props.onPress();
  assert.equal(f.retries(), 1);
  f.opened();
  assert.equal(find(f.render(), 'deckcraft-retry-open'), undefined);
  button.props.onPress();
  assert.equal(f.retries(), 1);
  f.close();
  assert.equal(f.owner().retired, true);
  button.props.onPress();
  assert.equal(f.retries(), 1);
});
