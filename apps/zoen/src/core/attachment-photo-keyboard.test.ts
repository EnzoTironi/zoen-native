import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';
import {
  AttachmentPhotoSession,
  type PhotoPickerState,
} from '../media/attachment-photo-session.ts';
import type { PhotoLibraryItem } from '../media/attachment-photo-library';
const compile = async (name: string) =>
  (
    await build({
      stdin: { contents: readFileSync(new URL(name, import.meta.url), 'utf8'), loader: 'tsx' },
      bundle: true,
      platform: 'node',
      format: 'cjs',
      jsx: 'automatic',
      write: false,
      plugins: [
        {
          name: 'actual-native-panel-ports',
          setup(b) {
            b.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
          },
        },
      ],
    })
  ).outputFiles[0]!.text;
const [panelCode, nativeCode] = await Promise.all([
  compile('../media/attachment-photo-keyboard.ios.tsx'),
  compile('../media/keyboard-panel-native.ios.tsx'),
]);
const tick = async () => {
  for (let i = 0; i < 14; i++) await Promise.resolve();
};
const item = (id: string): PhotoLibraryItem =>
  ({
    id: `ph://${id}`,
    filename: `${id}.jpg`,
    mediaType: 'image',
    width: 100,
    height: 200,
    duration: null,
    creationTime: null,
    modificationTime: null,
    isFavorite: false,
  }) as PhotoLibraryItem;
async function fixture() {
  let sequence = 0,
    slotIndex = 0,
    current = true,
    preferred: number | null = null;
  const calls: string[] = [],
    copies: string[][] = [],
    captures: number[] = [];
  const keyboard = new AttachmentKeyboardSession('space:thread', true, () => `owner-${++sequence}`);
  keyboard.bind({ tag: 7, context: 'space:normal', label: 'Message', placeholder: 'Message' });
  const photos = new AttachmentPhotoSession({
    permission: async () => ({
      granted: true,
      status: 'granted' as never,
      canAskAgain: true,
      accessPrivileges: 'all',
    }),
    requestPermission: async () => {
      throw new Error('not requested at mount');
    },
    load: async () => [item('a'), item('b'), item('c')],
    albums: async () => [{ id: 'album', title: 'Album' }],
    manage: async () => {},
    materialize: async (values) => {
      copies.push(values.map((v) => v.id));
      return values.map((v) => ({
        uri: `file:///owned/${v.id.slice(5)}`,
        mime: 'image/jpeg',
        name: v.filename!,
        size: 12,
      }));
    },
    capture: async (remaining) => {
      captures.push(remaining);
      return [{ uri: 'file:///owned/camera', mime: 'image/jpeg', name: 'camera.jpg', size: 12 }];
    },
    discard: () => {},
  });
  let picked: unknown;
  keyboard.register(async (kind) => {
    calls.push(kind);
    if (kind === 'photo') {
      keyboard.admitPicker(keyboard.getSnapshot().presentation!.id);
      picked = await photos.open(3, () => current);
      await Promise.resolve();
      calls.push('photo-drained');
    }
  }, true);
  keyboard.openAttachments();
  await tick();
  const request = keyboard.getSnapshot().presentation!;
  type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  let effects: (() => void)[] = [];
  const same = (a: readonly unknown[] | undefined, b: readonly unknown[]) =>
    !!a && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const jsx = (type: unknown, props: Record<string, any>) => ({ type, props });
  const ports: Record<string, unknown> = {
    react: {
      useSyncExternalStore: (_s: unknown, get: () => unknown) => get(),
      useMemo: (read: () => unknown, deps: readonly unknown[]) => {
        const s = (slots[slotIndex++] ??= {});
        if (!same(s.deps, deps)) {
          s.value = read();
          s.deps = deps;
        }
        return s.value;
      },
      useRef: (initial: unknown) => {
        const s = (slots[slotIndex++] ??= {});
        return (s.value ??= { current: initial });
      },
      useLayoutEffect: (read: () => void | (() => void), deps: readonly unknown[]) => {
        const s = (slots[slotIndex++] ??= {});
        if (same(s.deps, deps)) return;
        s.deps = deps;
        effects.push(() => {
          s.cleanup?.();
          s.cleanup = read() ?? undefined;
        });
      },
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': {
      useWindowDimensions: () => ({ width: 402, height: 874 }),
      Linking: { openSettings: () => calls.push('settings') },
      Keyboard: { metrics: () => ({ height: 336 }) },
    },
    expo: { requireNativeView: (_module: string, name: string) => name },
    'expo-crypto': { randomUUID: () => `render-${++sequence}` },
    'expo-haptics': { selectionAsync: async () => calls.push('haptic') },
    'expo-glass-effect': { isLiquidGlassAvailable: () => true },
    '../theme': {
      useTheme: () => ({
        presentation: {
          actionSheet: { background: '#000000' },
          text: '#ffffff',
          secondary: '#aaaaaa',
          accent: '#0088ff',
          separator: '#333333',
        },
      }),
    },
    '../i18n': { t: (name: string) => name },
  };
  const load = (code: string) => {
    const module = { exports: {} as any };
    runInNewContext(code, {
      module,
      exports: module.exports,
      require: (name: string) => {
        assert.ok(name in ports, name);
        return ports[name];
      },
    });
    return module.exports;
  };
  ports['./keyboard-panel-native.ios'] = load(nativeCode);
  const component = load(panelCode).AttachmentPhotoKeyboard;
  const render = (state: PhotoPickerState = photos.getSnapshot()) => {
    slotIndex = 0;
    effects = [];
    const tree = component({
      state,
      session: photos,
      keyboard,
      request,
      onPreferredIndex: (value: number | null) => {
        preferred = value;
      },
    });
    effects.forEach((run) => run());
    return tree as { type: string; props: Record<string, any> };
  };
  const act = (tree: ReturnType<typeof render>, action: string, id?: string) =>
    tree.props.onAction({
      nativeEvent: {
        token: request.id,
        context: request.field.context,
        revision: JSON.parse(tree.props.model).revision,
        action,
        id,
      },
    });
  const close = (tree: ReturnType<typeof render>) =>
    tree.props.onClosed({
      nativeEvent: { token: request.id, context: request.field.context, reason: 'closed' },
    });
  return {
    photos,
    keyboard,
    request,
    render,
    act,
    close,
    calls,
    copies,
    captures,
    picked: () => picked,
    preferred: () => preferred,
    retire: () => {
      current = false;
      keyboard.retire();
      photos.cancel();
    },
  };
}
test('actual native projection carries ordered selection/permission/palette and exact field binding without opening a sheet', async () => {
  const f = await fixture();
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  const tree = f.render(),
    binding = JSON.parse(tree.props.binding),
    model = JSON.parse(tree.props.model);
  assert.equal(tree.type, 'ZoenKeyboardPanelHost');
  assert.equal(binding.tag, 7);
  assert.equal(binding.context, 'space:normal');
  assert.equal(binding.preferredHeight, 336);
  assert.equal(binding.active, true);
  assert.equal(binding.appearance.background, '#000000');
  assert.deepEqual(model.selected, ['ph://b', 'ph://a']);
  assert.equal(model.columns, 3);
  assert.equal(model.hasAudio, true);
  f.retire();
  await tick();
});
test('queued same-token old model actions cannot override newer album/selection projection', async () => {
  const f = await fixture(),
    old = f.render();
  f.photos.select(item('b'));
  f.render();
  f.act(old, 'select', 'ph://a');
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((i) => i.id),
    ['ph://b'],
  );
  f.act(old, 'preview', 'ph://a');
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((i) => i.id),
    ['ph://b'],
  );
  assert.deepEqual(f.copies, []);
  f.retire();
  await tick();
});
test('retained native camera close consults CURRENT session state, waits removal then captures once', async () => {
  const f = await fixture(),
    tree = f.render();
  f.act(tree, 'camera');
  assert.equal(f.photos.getSnapshot().visible, false);
  assert.deepEqual(f.captures, []);
  // Intentionally no React render: callback still closes over visible=true.
  f.close(tree);
  await tick();
  assert.deepEqual(f.captures, [3]);
  assert.equal((f.picked() as any[])[0].uri, 'file:///owned/camera');
  assert.deepEqual(f.calls, ['photo', 'photo-drained']);
});
test('preview admits selected order and preferred index, but releases files only after native close', async () => {
  const f = await fixture();
  f.photos.select(item('b'));
  const tree = f.render();
  f.act(tree, 'preview', 'ph://a');
  await tick();
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  assert.equal(f.preferred(), 1);
  assert.equal(f.picked(), undefined);
  f.close(tree);
  await tick();
  assert.deepEqual(
    (f.picked() as any[]).map((i) => i.uri),
    ['file:///owned/b', 'file:///owned/a'],
  );
});
test('Files handoff waits native close plus the real Photos task finally; selection hides category admission', async () => {
  const f = await fixture(),
    tree = f.render();
  f.act(tree, 'document');
  assert.deepEqual(f.calls, ['photo']);
  assert.equal(f.keyboard.getSnapshot().presentation?.closing, true);
  f.close(tree);
  await tick();
  assert.deepEqual(f.calls, ['photo', 'photo-drained', 'document']);
  const g = await fixture();
  g.photos.select(item('a'));
  const selected = g.render();
  g.act(selected, 'audio');
  assert.equal(g.keyboard.getSnapshot().presentation?.closing, false);
  g.retire();
  await tick();
});
test('foreign action identity and retired ownership cannot select, prepare or handoff', async () => {
  const f = await fixture(),
    tree = f.render();
  tree.props.onAction({
    nativeEvent: {
      token: 'other',
      context: f.request.field.context,
      revision: JSON.parse(tree.props.model).revision,
      action: 'select',
      id: 'ph://a',
    },
  });
  f.retire();
  f.act(tree, 'preview', 'ph://a');
  f.close(tree);
  await tick();
  assert.deepEqual(f.copies, []);
  assert.deepEqual(f.captures, []);
  assert.deepEqual(f.calls, ['photo', 'photo-drained']);
});

test('native account retirement cannot approve pending Camera or category handoff', async () => {
  for (const action of ['camera', 'document']) {
    const f = await fixture(),
      tree = f.render();
    f.act(tree, action);
    tree.props.onClosed({
      nativeEvent: { token: f.request.id, context: f.request.field.context, reason: 'retired' },
    });
    await tick();
    assert.deepEqual(f.captures, []);
    assert.deepEqual(f.calls, ['photo', 'photo-drained']);
    assert.equal(f.keyboard.getSnapshot().presentation, null);
  }
});
