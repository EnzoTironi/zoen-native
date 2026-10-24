import test from 'node:test';
import { binaryEditorFormat } from './binary-formats.ts';
import { homeSpacesFor } from '../home/access.ts';
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
function findType(tree: any, type: string): Element | undefined {
  if (Array.isArray(tree)) return tree.flatMap((v) => findType(v, type) ?? [])[0];
  if (!tree?.props) return;
  return tree.type === type ? tree : findType(tree.props.children, type);
}
function fixture(historical = false, editor: 'deckcraft' | 'gridcraft' = 'deckcraft') {
  const slots: any[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  let retries = 0;
  let owner: any;
  let controller: any;
  let flushes = 0;
  const shares: string[] = [];
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
    '../home/access.ts': { homeSpacesFor },
    '../core/provider': { useClient: () => ({ store }) },
    '../components/primitives': {
      Loading: 'Loading',
      NativeButton: 'NativeButton',
      Surface: 'Surface',
    },
    '../theme': { useTheme: () => theme },
    './app-sheet-context': { useAppSheetCloseButton: () => null },
    './binary-formats': { binaryEditorFormat },
    './deckcraft-export-files': {},
    './frame': { MiniappFrame: 'MiniappFrame' },
    './resource-load': {},
    './sandbox': { sandboxHtml: () => '<p>Owned</p>' },
    './lifetime': { frameAuthority: (check: () => void) => ({ check, close: () => {} }) },
    './deckcraft-session': {
      createDeckcraftSession: (options: any) => {
        const current = {
          share: async (format: string) => {
            shares.push(format);
          },
          opened: false,
          retired: false,
          controller: {
            flush: async () => {
              flushes++;
            },
          },
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
    selectedHead: historical ? { version: '1', sha256: 'a'.repeat(64) } : undefined,
    item: { id: 'owned', title: 'Owned' },
    fill: true,
    verified: { resource: { text: '<p>Owned</p>' }, manifest: { id: editor } },
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
    shares,
    select: () => {
      props.selectedHead = { version: '2', sha256: 'b'.repeat(64) };
    },
    owner: () => owner,
    feedback: (state: unknown) => owner.state(state),
    flushes: () => flushes,
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

test('compiled opened save warning leaves input available; retry and actual recovery remain gated by the same mounted owner', () => {
  const f = fixture();
  try {
    f.opened();
    const owner = f.owner();
    f.feedback({
      phase: 'error',
      opened: true,
      blocked: false,
      error: 'Finish this edit before saving.',
      reason: 'autosave_busy',
    });
    let tree = f.render();
    assert.equal(find(tree, 'deckcraft-interaction-cover'), undefined);
    assert.ok(find(tree, 'deckcraft-save-warning'));
    assert.equal(find(tree, 'deckcraft-retry-open'), undefined);
    assert.equal(find(tree, 'deckcraft-save-status')?.props.children, 'decks.pending');
    find(tree, 'deckcraft-warning-retry-save')!.props.onPress();
    assert.equal(f.flushes(), 1);
    assert.equal(f.owner(), owner);
    assert.equal(owner.retired, false);
    // A retry request is not a save/resume ACK and cannot dismiss the warning.
    assert.ok(find(f.render(), 'deckcraft-save-warning'));
    f.feedback({
      phase: 'error',
      opened: true,
      blocked: false,
      error: 'The presentation view could not update.',
      reason: 'layout_failed',
    });
    assert.ok(find(f.render(), 'deckcraft-save-warning'));
    assert.equal(find(f.render(), 'deckcraft-interaction-cover'), undefined);
    // A failed view layout is not a save intent or a reason to remount a draft.
    assert.equal(find(f.render(), 'deckcraft-warning-retry-save'), undefined);
    assert.equal(f.flushes(), 1);
    f.feedback({
      phase: 'error',
      opened: true,
      blocked: true,
      error: 'A real final save has failed.',
      reason: 'flush_failed',
    });
    tree = f.render();
    assert.ok(find(tree, 'deckcraft-interaction-cover'));
    assert.equal(find(tree, 'deckcraft-save-warning'), undefined);
    assert.equal(f.owner(), owner);
    assert.equal(owner.retired, false);
    f.opened();
    assert.equal(find(f.render(), 'deckcraft-interaction-cover'), undefined);
    assert.equal(find(f.render(), 'deckcraft-save-warning'), undefined);
    assert.equal(f.retries(), 0);
  } finally {
    f.close();
  }
});

test('actual compiled native-frame failure remains covered after late guest saved status and cannot pretend a resume repairs the frame', () => {
  const f = fixture();
  try {
    f.opened();
    const owner = f.owner();
    findType(f.render(), 'MiniappFrame')!.props.onError('private native URL and path');
    let tree = f.render();
    assert.equal(findType(tree, 'MiniappFrame')!.props.blocked, true);
    assert.ok(find(tree, 'deckcraft-interaction-cover'));
    assert.equal(find(tree, 'deckcraft-save-warning'), undefined);
    assert.equal(find(tree, 'deckcraft-recovery-retry-save'), undefined);
    assert.equal(find(tree, 'deckcraft-retry-open'), undefined);
    assert.equal(JSON.stringify(tree).includes('private native URL'), false);
    f.opened();
    tree = f.render();
    assert.ok(find(tree, 'deckcraft-interaction-cover'));
    assert.equal(findType(tree, 'MiniappFrame')!.props.blocked, true);
    assert.equal(find(tree, 'deckcraft-save-status')?.props.children, 'decks.pending');
    assert.equal(f.owner(), owner);
    assert.equal(owner.retired, false);
    assert.equal(f.flushes(), 0);
    assert.equal(f.retries(), 0);
  } finally {
    f.close();
  }
});

test('compiled historical controls export from the displayed owner and stale buttons cannot target a replacement version or closed frame', () => {
  const f = fixture(true);
  f.opened();
  const pdf = find(f.render(), 'deckcraft-history-export-pdf')!;
  const pptx = find(f.render(), 'deckcraft-history-export-pptx')!;
  assert.ok(pdf);
  assert.ok(pptx);
  pdf.props.onPress();
  pptx.props.onPress();
  assert.deepEqual(f.shares, ['pdf', 'pptx']);
  f.select();
  f.render();
  pdf.props.onPress();
  pptx.props.onPress();
  assert.deepEqual(f.shares, ['pdf', 'pptx']);
  f.opened();
  const replacement = find(f.render(), 'deckcraft-history-export-pdf')!;
  replacement.props.onPress();
  assert.deepEqual(f.shares, ['pdf', 'pptx', 'pdf']);
  f.close();
  replacement.props.onPress();
  assert.deepEqual(f.shares, ['pdf', 'pptx', 'pdf']);
});

test('compiled Grid history offers only XLSX and keeps retired controls bound to their original frame', () => {
  const f = fixture(true, 'gridcraft');
  try {
    f.opened();
    const tree = f.render();
    assert.equal(find(tree, 'gridcraft-history-export-pdf'), undefined);
    assert.equal(find(tree, 'gridcraft-history-export-pptx'), undefined);
    const xlsx = find(tree, 'gridcraft-history-export-xlsx')!;
    assert.ok(xlsx);
    assert.equal(xlsx.props.label, 'Excel');
    xlsx.props.onPress();
    assert.deepEqual(f.shares, ['xlsx']);
    f.select();
    f.render();
    xlsx.props.onPress();
    assert.deepEqual(f.shares, ['xlsx']);
  } finally {
    f.close();
  }
});
