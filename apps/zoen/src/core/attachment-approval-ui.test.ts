import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { AttachmentApprovalProps } from '../media/attachment-approval.types.ts';

type Element = { type: unknown; props: Record<string, unknown> };
const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../media/attachment-approval.ios.tsx', import.meta.url),
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
      name: 'approval-native-view-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
function scan(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(scan);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const e = value as Element;
  return [e, ...scan(e.props.children), ...scan(e.props.overlay)];
}
function find(root: unknown, id: string) {
  const result = scan(root).find((e) => e.props.testID === id);
  assert.ok(result, id);
  return result;
}
function fixture() {
  type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  const slot = () => (slots[cursor++] ??= {});
  const effectsFor = (effect: () => void | (() => void), deps: readonly unknown[]) => {
    const s = slot();
    if (s.deps && s.deps.length === deps.length && deps.every((v, i) => Object.is(v, s.deps![i])))
      return;
    s.deps = deps;
    effects.push(() => {
      s.cleanup?.();
      s.cleanup = effect() ?? undefined;
    });
  };
  const calls: string[] = [];
  let current = true,
    width = 402,
    height = 874;
  let safe = { top: 62, left: 0, right: 0, bottom: 34 };
  const items = [
    {
      mutationId: 'photo',
      mime: 'image/jpeg',
      uri: 'file:///owned/photo',
      name: 'Original photo',
      caption: 'Photo caption',
      attempted: false,
      size: 10,
    },
    {
      mutationId: 'video',
      mime: 'video/mp4',
      uri: 'file:///owned/video',
      name: 'Original video',
      caption: 'Video caption',
      attempted: false,
      size: 20,
    },
  ];
  const scope = {
    mode: 'account' as const,
    identityId: 'me',
    deviceId: 'device',
    spaceId: 'space',
    replyTo: 'original-reply',
    thread: true,
  };
  const fieldFor = (index: number) => ({
    scope,
    targetFrozen: true,
    value: items[index]!.caption,
    position: index + 1,
    total: 2,
    editable: true,
    sending: false,
    remaining: 2,
    retry: false,
    onChange: (value: string) => calls.push(`caption:${items[index]!.mutationId}:${value}`),
    onSend: () => calls.push('explicit-send'),
  });
  const props = {
    testID: 'approval',
    state: { phase: 'ready', items },
    field: fieldFor(0),
    selectedId: 'photo',
    recipient: 'Original recipient',
    busy: false,
    editing: false,
    canAdd: true,
    canEdit: true,
    error: null,
    isCurrent: () => current,
    onClose: () => calls.push('close-preserve'),
    onSelect: (id: string) => calls.push(`select:${id}`),
    onEdit: () => calls.push('edit'),
    onAdd: (kind: string) => calls.push(`add:${kind}`),
    onRemove: (id: string) => calls.push(`remove:${id}`),
    onRetry: () => calls.push('retry'),
  } as unknown as AttachmentApprovalProps;
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const ports: Record<string, unknown> = {
    react: {
      useRef: (initial: unknown) => {
        const s = slot();
        s.value ??= { current: initial };
        return s.value;
      },
      useState: (initial: unknown) => {
        const s = slot();
        if (!Object.hasOwn(s, 'value')) s.value = initial;
        return [
          s.value,
          (value: unknown) => {
            s.value =
              typeof value === 'function' ? (value as (v: unknown) => unknown)(s.value) : value;
          },
        ];
      },
      useEffect: effectsFor,
      useLayoutEffect: effectsFor,
    },
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'react-native': {
      ActivityIndicator: 'ActivityIndicator',
      Keyboard: { dismiss: () => calls.push('keyboard-dismiss') },
      Modal: 'Modal',
      Pressable: 'Pressable',
      ScrollView: 'ScrollView',
      Text: 'Text',
      View: 'View',
      useWindowDimensions: () => ({ width, height, scale: 3 }),
    },
    '@expo/ui': { Host: 'Host' },
    '@expo/ui/swift-ui': {
      Button: 'Button',
      Menu: 'Menu',
      TextField: 'TextField',
      useNativeState: (value: string) => ({ value, set() {} }),
    },
    '@expo/ui/swift-ui/modifiers': new Proxy(
      {},
      {
        get:
          (_target, name) =>
          (...args: unknown[]) => ({ name, args }),
      },
    ),
    expo: { useEvent() {}, useEventListener() {} },
    'expo-image': { Image: 'Image' },
    'expo-glass-effect': { isLiquidGlassAvailable: () => true },
    'expo-status-bar': { StatusBar: 'StatusBar' },
    'expo-haptics': {
      selectionAsync: () => {
        calls.push('haptic');
        return Promise.resolve();
      },
    },
    'expo-video': { useVideoPlayer() {}, VideoView: 'VideoView' },
    'react-native-keyboard-controller': { KeyboardAvoidingView: 'KeyboardAvoidingView' },
    'react-native-safe-area-context': {
      SafeAreaProvider: 'SafeAreaProvider',
      SafeAreaView: 'SafeAreaView',
      useSafeAreaInsets: () => safe,
    },
    '../components/primitives': { Icon: 'Icon' },
    './attachment-draft-strip': { DraftThumbnail: 'DraftThumbnail' },
    './attachment-gallery-pager': { AttachmentGalleryPager: 'AttachmentGalleryPager' },
    './attachment-image-surface': { AttachmentImageSurface: 'AttachmentImageSurface' },
    './attachment-gallery-surface': { AttachmentGallerySurface: 'AttachmentGallerySurface' },
    './playback-owner': { mediaPlayback: { pauseAll: () => calls.push('pause-previews') } },
    '../i18n': { t: (key: string) => key },
    '../theme': {
      useTheme: () => ({ reducedMotion: false, colors: { accent: '#0088ff', onAccent: '#fff' } }),
    },
  };
  const module = { exports: {} as { AttachmentApproval(props: AttachmentApprovalProps): Element } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const presentation = module.exports.AttachmentApproval(props);
  const content = scan(presentation).find(
    (e) => typeof e.type === 'function' && e.type.name === 'ApprovalContent',
  );
  assert.ok(content);
  const render = (patch: Partial<AttachmentApprovalProps> = {}) => {
    Object.assign(props, patch);
    cursor = 0;
    effects = [];
    const root = (content.type as (props: AttachmentApprovalProps) => Element)(props);
    for (const effect of effects) effect();
    return root;
  };
  return {
    render,
    calls,
    props,
    fieldFor,
    presentation,
    landscape: (side: 'left' | 'right') => {
      width = 874;
      height = 402;
      safe = {
        top: 0,
        bottom: 21,
        left: side === 'left' ? 62 : 0,
        right: side === 'right' ? 62 : 0,
      };
    },
    retire: () => {
      current = false;
    },
  };
}

test('actual approval delegates page selection, per-item caption and explicit Send without copying media or posting on navigation', () => {
  const f = fixture();
  let root = f.render();
  const pager = scan(root).find((e) => e.type === 'AttachmentGalleryPager')!;
  assert.equal(pager.props.items, f.props.state.items);
  assert.equal(pager.props.selectedId, 'photo');
  (pager.props.onSelect as (id: string) => void)('video');
  assert.deepEqual(f.calls, ['haptic', 'select:video']);
  root = f.render({ selectedId: 'video', field: f.fieldFor(1) });
  const caption = scan(root).find(
    (e) => typeof e.type === 'function' && e.type.name === 'Caption',
  )!;
  assert.equal(caption.props.field, f.props.field);
  assert.equal(f.props.field.value, 'Video caption');
  f.props.field.onChange('Exact edited caption');
  assert.deepEqual(f.calls, ['haptic', 'select:video', 'caption:video:Exact edited caption']);
  (find(root, 'attachment-approval-send').props.onPress as () => void)();
  assert.equal(f.calls.at(-1), 'explicit-send');
  assert.equal(f.props.field.scope.replyTo, 'original-reply');
  assert.equal(f.props.field.scope.thread, true);
});

test('selection count reveals the actual ordered rail and retired UI cannot choose an asset', () => {
  const f = fixture();
  let root = f.render();
  assert.equal(
    scan(root).some((e) => e.props.testID === 'attachment-approval-selection-rail'),
    false,
  );
  (find(root, 'attachment-approval-selection-count').props.onPress as () => void)();
  root = f.render();
  (find(root, 'attachment-approval-thumbnail-1').props.onPress as () => void)();
  assert.deepEqual(f.calls, ['haptic', 'select:video']);
  f.retire();
  (find(root, 'attachment-approval-thumbnail-0').props.onPress as () => void)();
  assert.deepEqual(f.calls, ['haptic', 'select:video']);
  assert.deepEqual(
    f.props.state.items.map((item) => item.mutationId),
    ['photo', 'video'],
  );
});

test('only the selected video owns preview, drag pauses it and Close preserves the existing draft', () => {
  const f = fixture();
  const root = f.render();
  const pager = scan(root).find((e) => e.type === 'AttachmentGalleryPager')!;
  const item = f.props.state.items[1]!;
  const renderItem = pager.props.renderItem as (
    item: (typeof f.props.state.items)[number],
    active: boolean,
  ) => Element;
  assert.equal(typeof renderItem(item, true).type, 'function');
  assert.equal(renderItem(item, false).type, 'View');
  (pager.props.onDragStart as () => void)();
  assert.deepEqual(f.calls, ['keyboard-dismiss', 'pause-previews']);
  (find(root, 'attachment-approval-close').props.onPress as () => void)();
  assert.equal(f.calls.at(-1), 'close-preserve');
  assert.deepEqual(
    f.props.state.items.map((item) => item.uri),
    ['file:///owned/photo', 'file:///owned/video'],
  );
});

test('caption focus hides selection and edit tools while explicit send remains the same authoritative callback', () => {
  const f = fixture();
  let root = f.render();
  const caption = scan(root).find(
    (e) => typeof e.type === 'function' && e.type.name === 'Caption',
  )!;
  (caption.props.onFocus as (value: boolean) => void)(true);
  root = f.render();
  assert.equal(
    scan(root).some((e) => e.props.testID === 'attachment-approval-selection-count'),
    false,
  );
  assert.equal(
    scan(root).some((e) => e.props.testID === 'attachment-approval-edit'),
    false,
  );
  assert.equal(find(root, 'attachment-approval-send').props.onPress, f.props.field.onSend);
  assert.deepEqual(f.calls, []);
});

test('current photo zoom locks paging and dismissal; old photo chrome callbacks cannot lock a newly selected item', () => {
  const f = fixture();
  let root = f.render();
  const pager = () => scan(root).find((e) => e.type === 'AttachmentGalleryPager')!;
  const renderItem = pager().props.renderItem as (
    item: (typeof f.props.state.items)[number],
    active: boolean,
  ) => Element;
  const photo = renderItem(f.props.state.items[0]!, true);
  (photo.props.onZoomChange as (id: string, value: boolean) => void)('photo', true);
  root = f.render();
  assert.equal(pager().props.disabled, true);
  assert.equal(scan(root).find((e) => e.type === 'AttachmentGallerySurface')!.props.disabled, true);
  root = f.render({ selectedId: 'video', field: f.fieldFor(1) });
  root = f.render();
  (photo.props.onZoomChange as (id: string, value: boolean) => void)('photo', true);
  (photo.props.onToggleControls as () => void)();
  root = f.render();
  assert.equal(pager().props.disabled, false);
  assert.equal(
    scan(root).find((e) => e.type === 'AttachmentGallerySurface')!.props.controlsHidden,
    false,
  );
  root = f.render({ editing: true });
  assert.equal(pager().props.disabled, true);
  root = f.render({ editing: false });
  assert.equal(pager().props.disabled, false);
});

test('landscape uses the source 49-point side toolbar and vertical ordered rail on the safe-inset side without changing the field', () => {
  for (const side of ['left', 'right'] as const) {
    const f = fixture();
    f.landscape(side);
    let root = f.render();
    (find(root, 'attachment-approval-selection-count').props.onPress as () => void)();
    root = f.render();
    const rail = find(root, 'attachment-approval-selection-rail');
    assert.equal(rail.props.horizontal, false);
    assert.equal((rail.props.style as Record<string, unknown>)[side], 115);
    const toolbar = scan(root).find((e) => (e.props.style as { width?: number })?.width === 49)!;
    assert.ok(toolbar);
    assert.equal((toolbar.props.style as Record<string, unknown>).flexDirection, 'column');
    assert.equal((toolbar.props.style as Record<string, unknown>)[side], 0);
    assert.equal(find(root, 'attachment-approval-send').props.onPress, f.props.field.onSend);
    assert.deepEqual(
      f.props.state.items.map((item) => item.mutationId),
      ['photo', 'video'],
    );
  }
});
