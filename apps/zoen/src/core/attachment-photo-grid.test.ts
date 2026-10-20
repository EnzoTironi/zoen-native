import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import {
  AttachmentPhotoSession as ActualPhotoSession,
  type PhotoPickerState,
  type AttachmentPhotoSession,
} from '../media/attachment-photo-session.ts';
type Element = { type: unknown; props: Record<string, unknown> };
const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../media/attachment-photo-picker.ios.tsx', import.meta.url),
      'utf8',
    ),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'actual-grid-native-ports',
      setup(b) {
        b.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
function fixture() {
  let width = 402,
    height = 874,
    cursor = 0,
    current = true,
    haptics = 0;
  const slots: unknown[] = [];
  let effects: (() => void)[] = [];
  const calls: string[] = [];
  const colors = {
    background: '#000',
    surface: '#222',
    accent: '#af0',
    onAccent: '#000',
    text: '#fff',
    secondary: '#aaa',
    danger: '#f00',
  };
  const jsx = (type: unknown, props: Record<string, unknown>): Element => ({ type, props });
  const ports: Record<string, unknown> = {
    react: {
      useState: (initial: unknown) => {
        const i = cursor++;
        if (!(i in slots)) slots[i] = initial;
        return [
          slots[i],
          (value: unknown) => {
            slots[i] = value;
          },
        ];
      },
      useEffect: (effect: () => void) => {
        effects.push(effect);
      },
      useMemo: (read: () => unknown) => read(),
      useSyncExternalStore: () => {},
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': {
      FlatList: 'FlatList',
      View: 'View',
      Pressable: 'Pressable',
      Text: 'Text',
      ActivityIndicator: 'ActivityIndicator',
      PixelRatio: { get: () => 3 },
      useWindowDimensions: () => ({ width, height }),
      Linking: { openSettings: () => {} },
      AppState: {},
    },
    '@expo/ui': { Host: 'Host', BottomSheet: 'BottomSheet' },
    '@expo/ui/swift-ui': {
      BottomSheet: 'NativeBottomSheet',
      Group: 'Group',
      Button: 'Button',
      Menu: 'Menu',
      RNHostView: 'RNHostView',
    },
    '@expo/ui/swift-ui/modifiers': new Proxy({}, { get: () => () => {} }),
    'expo-image': { Image: 'Image' },
    'expo-haptics': {
      selectionAsync: () => {
        haptics++;
        return Promise.resolve();
      },
    },
    'expo-camera': {
      Camera: { getCameraPermissionsAsync: async () => ({ granted: true }) },
      CameraView: 'CameraView',
    },
    '../components/primitives': { Icon: 'Icon' },
    '../theme': { useTheme: () => ({ colors }) },
    '../i18n': { t: (s: string) => s },
    './attachment-photo-session': {},
    './attachment-photo-library': {},
    './attachment-picker': {},
  };
  const module = {
    exports: {} as {
      AttachmentPhotoPickerSheet(props: {
        state: PhotoPickerState;
        session: AttachmentPhotoSession;
      }): Element;
      AttachmentPhotoGrid(props: {
        state: PhotoPickerState;
        session: AttachmentPhotoSession;
      }): Element;
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (id: string) => {
      assert.ok(id in ports, id);
      return ports[id];
    },
  });
  const state = {
    token: 1,
    visible: true,
    busy: false,
    permission: {
      granted: true,
      status: 'granted' as NonNullable<PhotoPickerState['permission']>['status'],
      canAskAgain: true,
    },
    items: Array.from({ length: 10 }, (_, i) => ({
      id: String(i),
      filename: `${i}.jpg`,
      mediaType: 'image',
      width: 100,
      height: 200,
      duration: null,
      creationTime: null,
      modificationTime: null,
      isFavorite: false,
    })),
    selected: ['2', '0'].map((id) => ({
      id,
      filename: id + '.jpg',
      mediaType: 'image',
      width: 100,
      height: 200,
      duration: null,
      creationTime: null,
      modificationTime: null,
      isFavorite: false,
    })),
    albums: [],
    albumId: null,
    loading: false,
    hasMore: false,
    error: null,
  } as { -readonly [K in keyof PhotoPickerState]: PhotoPickerState[K] };
  const session = {
    isCurrent: () => current,
    isInteractive: () => current && state.visible && !state.busy,
    isPresented: () => current && state.visible,
    select: (i: { id: string }) => calls.push(`select:${i.id}`),
    camera: () => calls.push('camera'),
    dismissing: (token: number) => calls.push(`dismissing:${token}`),
    dismissed: async (token: number) => {
      calls.push(`dismissed:${token}`);
    },
    more: () => {
      calls.push('more');
      return Promise.resolve();
    },
  } as unknown as AttachmentPhotoSession;
  const render = () => {
    cursor = 0;
    effects = [];
    const root = module.exports.AttachmentPhotoGrid({ state, session });
    effects.forEach((f) => f());
    return root;
  };
  const scan = (value: unknown): Element[] =>
    Array.isArray(value)
      ? value.flatMap(scan)
      : value && typeof value === 'object' && 'props' in value
        ? [
            value as Element,
            ...scan((value as Element).props.children),
            ...scan((value as Element).props.ListHeaderComponent),
          ]
        : [];
  return {
    render,
    sheet: () => module.exports.AttachmentPhotoPickerSheet({ state, session }),
    sheetFor: (state: PhotoPickerState, session: AttachmentPhotoSession) =>
      module.exports.AttachmentPhotoPickerSheet({ state, session }),
    state,
    calls,
    scan,
    find: (root: Element, id: string) => {
      const e = scan(root).find((e) => e.props.testID === id);
      assert.ok(e, id);
      return e;
    },
    rotate: () => {
      width = 874;
      height = 402;
    },
    retire: () => {
      current = false;
    },
    haptics: () => haptics,
  };
}
test('actual grid uses source three-column one-point geometry with a true two-row camera cutout and exact ordered badges', () => {
  const f = fixture(),
    root = f.render();
  assert.equal(root.props.numColumns, 3);
  const camera = f.find(root, 'attachment-photos-camera');
  const style = camera.props.style as { width: number; height: number };
  const size = Math.floor(((402 - 2) / 3) * 3) / 3;
  assert.equal(style.width, size);
  assert.equal(style.height, size * 2 + 1);
  assert.equal((root.props.data as unknown[]).length, 6);
  const two = f.find(root, 'attachment-photo-2');
  assert.ok(f.scan(two).some((e) => e.type === 'Text' && e.props.children === 1));
  const zero = f.find(root, 'attachment-photo-0');
  assert.ok(f.scan(zero).some((e) => e.type === 'Text' && e.props.children === 2));
  (two.props.onPress as () => void)();
  (camera.props.onPress as () => void)();
  assert.deepEqual(f.calls, ['select:2', 'camera']);
  assert.equal(f.haptics(), 1);
});
test('rotation preserves ordered selections and converges to actual presented width with five landscape columns', () => {
  const f = fixture();
  let root = f.render();
  f.rotate();
  root = f.render();
  assert.equal(root.props.numColumns, 5);
  (root.props.onLayout as (e: unknown) => void)({ nativeEvent: { layout: { width: 800 } } });
  root = f.render();
  const camera = f.find(root, 'attachment-photos-camera');
  assert.equal(
    (camera.props.style as { width: number }).width,
    Math.floor(((800 - 4) / 5) * 3) / 3,
  );
  assert.deepEqual(
    Array.from(f.state.selected, (i) => i.id),
    ['2', '0'],
  );
});
test('retired grid callbacks cannot select, capture, paginate or emit haptics; preview mounts only for a visible idle library', async () => {
  const f = fixture(),
    root = f.render();
  await Promise.resolve();
  const active = f.render();
  assert.ok(f.scan(active).some((e) => e.type === 'CameraView'));
  const tile = f.find(root, 'attachment-photo-0'),
    camera = f.find(root, 'attachment-photos-camera');
  f.retire();
  (tile.props.onPress as () => void)();
  (camera.props.onPress as () => void)();
  (root.props.onEndReached as () => void)();
  assert.deepEqual(f.calls, []);
  assert.equal(f.haptics(), 0);
  f.state.busy = true;
  assert.ok(!f.scan(f.render()).some((e) => e.type === 'CameraView'));
  f.state.busy = false;
  f.state.visible = false;
  assert.ok(!f.scan(f.render()).some((e) => e.type === 'CameraView'));
});

test('actual picker hook never observes Photos at chat mount and removes its native observer when hidden or retired', () => {
  type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  let cursor = 0,
    effects: (() => void)[] = [];
  let observing = 0,
    stopped = 0,
    canceled = 0;
  let background: ((value: string) => void) | undefined;
  let snapshot = { visible: false, permission: null } as PhotoPickerState;
  const same = (a: readonly unknown[] | undefined, b: readonly unknown[]) =>
    Boolean(a && a.length === b.length && b.every((v, i) => Object.is(v, a[i])));
  class Session {
    subscribe = () => () => {};
    getSnapshot = () => snapshot;
    invalidate = () => {};
    cancel = () => {
      canceled++;
    };
    open = async () => [];
  }
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const ports: Record<string, unknown> = {
    react: {
      useMemo: (read: () => unknown, deps: readonly unknown[]) => {
        const slot = (slots[cursor++] ??= {});
        if (!same(slot.deps, deps)) {
          slot.deps = deps;
          slot.value = read();
        }
        return slot.value;
      },
      useSyncExternalStore: (_subscribe: unknown, get: () => unknown) => get(),
      useEffect: (run: () => void | (() => void), deps: readonly unknown[]) => {
        const slot = (slots[cursor++] ??= {});
        if (same(slot.deps, deps)) return;
        slot.deps = deps;
        effects.push(() => {
          slot.cleanup?.();
          slot.cleanup = run() ?? undefined;
        });
      },
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': {
      AppState: {
        addEventListener: (_event: string, listener: (state: string) => void) => {
          background = listener;
          return {
            remove: () => {
              background = undefined;
            },
          };
        },
      },
    },
    './attachment-photo-session': { AttachmentPhotoSession: Session },
    './attachment-photo-library': {
      photoLibrary: {
        subscribe: () => {
          observing++;
          return () => {
            stopped++;
          };
        },
      },
    },
    './attachment-picker': {},
    '@expo/ui': {},
    '@expo/ui/swift-ui': {},
    '@expo/ui/swift-ui/modifiers': {},
    'expo-image': {},
    'expo-haptics': {},
    'expo-camera': {},
    '../components/primitives': {},
    '../theme': {},
    '../i18n': {},
  };
  const module = { exports: {} as { useAttachmentPhotoPicker(): unknown } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (id: string) => {
      assert.ok(id in ports, id);
      return ports[id];
    },
  });
  const render = () => {
    cursor = 0;
    effects = [];
    module.exports.useAttachmentPhotoPicker();
    effects.forEach((run) => run());
  };
  render();
  assert.equal(observing, 0);
  assert.equal(canceled, 0);
  snapshot = { ...snapshot, visible: true };
  render();
  assert.equal(observing, 0);
  snapshot = {
    ...snapshot,
    permission: { granted: true } as NonNullable<PhotoPickerState['permission']>,
  };
  render();
  assert.equal(observing, 1);
  render();
  assert.equal(observing, 1);
  background?.('inactive');
  assert.equal(canceled, 0);
  background?.('background');
  assert.equal(canceled, 1);
  snapshot = { ...snapshot, visible: false };
  render();
  assert.equal(stopped, 1);
  slots.forEach((slot) => slot.cleanup?.());
  assert.equal(stopped, 1);
  assert.equal(canceled, 2);
  assert.equal(background, undefined);
});

test('actual SwiftUI sheet uses distinct state-change and fully-dismissed callback ports', () => {
  const f = fixture(),
    root = f.sheet();
  const sheet = f.scan(root).find((e) => e.type === 'NativeBottomSheet');
  assert.ok(sheet);
  (sheet.props.onIsPresentedChange as (value: boolean) => void)(false);
  assert.deepEqual(f.calls, ['dismissing:1']);
  (sheet.props.onDismiss as () => void)();
  assert.deepEqual(f.calls, ['dismissing:1', 'dismissed:1']);
});

test('actual sheet callbacks drive the real session: only completed dismissal captures, and retirement in between cancels', async () => {
  const f = fixture();
  let current = true,
    captures = 0;
  const session = new ActualPhotoSession({
    permission: async () => ({
      granted: true,
      status: 'granted' as NonNullable<PhotoPickerState['permission']>['status'],
      canAskAgain: true,
    }),
    requestPermission: async () => {
      throw new Error('unexpected prompt');
    },
    load: async () => [],
    albums: async () => [],
    manage: async () => {},
    materialize: async () => [],
    capture: async () => {
      captures++;
      return [{ uri: 'file:///cache/camera', name: 'capture.jpg', mime: 'image/jpeg', size: 10 }];
    },
    discard: () => {},
  });
  const tick = async () => {
    for (let i = 0; i < 12; i++) await Promise.resolve();
  };
  const first = session.open(2, () => current);
  await tick();
  session.camera();
  let sheet = f
    .scan(f.sheetFor(session.getSnapshot(), session))
    .find((e) => e.type === 'NativeBottomSheet')!;
  (sheet.props.onIsPresentedChange as (v: boolean) => void)(false);
  assert.equal(captures, 0);
  current = false;
  (sheet.props.onDismiss as () => void)();
  await tick();
  assert.deepEqual(await first, []);
  assert.equal(captures, 0);
  current = true;
  const second = session.open(2, () => current);
  await tick();
  session.camera();
  sheet = f
    .scan(f.sheetFor(session.getSnapshot(), session))
    .find((e) => e.type === 'NativeBottomSheet')!;
  (sheet.props.onIsPresentedChange as (v: boolean) => void)(false);
  assert.equal(captures, 0);
  (sheet.props.onDismiss as () => void)();
  await tick();
  const result = await second;
  assert.equal(captures, 1);
  assert.equal(result.length, 1);
  assert.equal(result[0]?.uri, 'file:///cache/camera');
});
