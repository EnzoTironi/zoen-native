import * as attachmentCategories from '../media/attachment-categories.ts';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';
import {
  AttachmentPhotoSession,
  type PhotoPickerState,
  type PhotoPickerSubmission,
} from '../media/attachment-photo-session.ts';
import type { PhotoLibraryItem } from '../media/attachment-photo-library';
import type { AttachmentAsset } from '../media/attachment-draft';
import { AttachmentRecentSession, recentAttachmentId } from '../media/attachment-recent-session.ts';
import { photoSendOptions } from '../media/attachment-photo-options.ts';
import type { MediaDraftAttachmentOptions } from '../media/media-draft-types';
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
const [panelCode, nativeCode, pickerCode] = await Promise.all([
  compile('../media/attachment-photo-keyboard.ios.tsx'),
  compile('../media/keyboard-panel-native.ios.tsx'),
  compile('../media/attachment-photo-picker.ios.tsx'),
]);
const tick = async () => {
  for (let i = 0; i < 14; i++) await Promise.resolve();
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
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
async function fixture(
  withSubmission = false,
  mediaItems: PhotoLibraryItem[] = [item('a'), item('b'), item('c')],
  recent?: AttachmentRecentSession,
  sendAsFileAllowed = false,
  existingOptions?: MediaDraftAttachmentOptions,
) {
  let sequence = 0,
    slotIndex = 0,
    current = true,
    preferred: number | null = null;
  const calls: string[] = [],
    copies: string[][] = [],
    captures: number[] = [];
  const submissions: PhotoPickerSubmission[] = [];
  const resumed: { id: string }[] = [];
  let capture: () => Promise<AttachmentAsset[]> = async () => [
    { uri: 'file:///owned/camera', mime: 'image/jpeg', name: 'camera.jpg', size: 12 },
  ];
  let browse: () => Promise<AttachmentAsset[]> = async () => [
    { uri: 'file:///owned/document', mime: 'application/pdf', name: 'document.pdf', size: 12 },
  ];
  let materialize: (values: readonly PhotoLibraryItem[]) => Promise<AttachmentAsset[]> = async (
    values,
  ) =>
    values.map((v) => ({
      uri: `file:///owned/${v.id.slice(5)}`,
      mime: v.mediaType === 'video' ? 'video/quicktime' : 'image/jpeg',
      name: v.filename!,
      size: 12,
    }));
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
    load: async () => mediaItems,
    refresh: async (items) => [...items],
    albums: async () => [{ id: 'album', title: 'Album' }],
    manage: async () => {},
    materialize: async (values) => {
      copies.push(values.map((v) => v.id));
      return materialize(values);
    },
    capture: async (remaining) => {
      captures.push(remaining);
      return capture();
    },
    browse: async () => {
      calls.push('browse');
      return browse();
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
  let request = keyboard.getSnapshot().presentation!;
  type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  let effects: (() => void)[] = [];
  const same = (a: readonly unknown[] | undefined, b: readonly unknown[]) =>
    !!a && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const jsx = (type: unknown, props: Record<string, any>) => ({ type, props });
  const ports: Record<string, unknown> = {
    './attachment-categories': attachmentCategories,
    './attachment-photo-options': { photoSendOptions },
    './attachment-recent-session': { AttachmentRecentSession, recentAttachmentId },
    '../../modules/zoen-core/src/ZoenCoreModule': { releaseLocalFile: async () => {} },
    react: {
      useSyncExternalStore: (_s: unknown, get: () => unknown) => get(),
      useState: (initial: unknown) => {
        const s = (slots[slotIndex++] ??= {});
        if (!Object.hasOwn(s, 'value')) s.value = initial;
        return [
          s.value,
          (value: unknown) => {
            s.value = typeof value === 'function' ? value(s.value) : value;
          },
        ];
      },
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
    '../i18n': {
      t: (name: string, values?: { count?: number }) =>
        name === 'media.attachmentSendCount' ? `${name}:${values?.count}` : name,
    },
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
  const render = (
    state: PhotoPickerState = photos.getSnapshot(),
    routes?: {
      categories: readonly ('article' | 'poll' | 'location' | 'contact')[];
      open(category: string, current: () => boolean): void;
    },
  ) => {
    slotIndex = 0;
    effects = [];
    const tree = component({
      state,
      session: photos,
      keyboard,
      request,
      sendAsFileAllowed,
      existingOptions,
      routes,
      ...(recent ? { recent: { session: recent, state: recent.getSnapshot() } } : {}),
      onPreferredIndex: (value: number | null) => {
        preferred = value;
      },
      onResume: (presentation: typeof request) => {
        request = presentation;
        resumed.push(presentation);
      },
      ...(withSubmission
        ? {
            onSubmission: (intent: PhotoPickerSubmission) => {
              submissions.push(intent);
            },
          }
        : {}),
    });
    effects.forEach((run) => run());
    return tree as { type: string; props: Record<string, any> };
  };
  const act = (
    tree: ReturnType<typeof render>,
    action: string,
    id?: string,
    payload: Record<string, unknown> = {},
  ) =>
    tree.props.onAction({
      nativeEvent: {
        token: JSON.parse(tree.props.binding).token,
        context: JSON.parse(tree.props.binding).context,
        revision: JSON.parse(tree.props.model).revision,
        action,
        id,
        ...payload,
      },
    });
  const close = (tree: ReturnType<typeof render>) =>
    tree.props.onClosed({
      nativeEvent: {
        token: JSON.parse(tree.props.binding).token,
        context: JSON.parse(tree.props.binding).context,
        reason: 'closed',
      },
    });
  return {
    photos,
    keyboard,
    get request() {
      return request;
    },
    render,
    act,
    close,
    calls,
    copies,
    captures,
    submissions,
    submission: (): PhotoPickerSubmission | undefined => submissions.at(-1),
    resumed,
    captureNext: (next: () => Promise<AttachmentAsset[]>) => {
      capture = next;
    },
    browseNext: (next: () => Promise<AttachmentAsset[]>) => {
      browse = next;
    },
    materializeNext: (next: typeof materialize) => {
      materialize = next;
    },
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

test('caption controls are projected only for the durable submission adapter', async () => {
  const ordinary = await fixture(),
    durable = await fixture(true);
  assert.equal('caption' in JSON.parse(ordinary.render().props.model), false);
  durable.photos.select(item('a'));
  const tree = durable.render();
  assert.equal(JSON.parse(tree.props.model).caption.count, 1);
  durable.act(tree, 'caption', undefined, { text: '  Caption 🖼️\nkept exactly  ' });
  assert.equal(
    JSON.parse(durable.render().props.model).caption.value,
    '  Caption 🖼️\nkept exactly  ',
  );
  assert.deepEqual(durable.submissions, []);
  ordinary.retire();
  durable.retire();
  await tick();
});

test('direct Send takes the final native text and awaits native removal before releasing selected files', async () => {
  const f = await fixture(true);
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  let tree = f.render();
  f.act(tree, 'caption', undefined, { text: 'Before the final keystroke' });
  tree = f.render();
  const finalCaption = 'Before the final keystroke!';
  f.act(tree, 'send', undefined, { text: finalCaption });
  // The mounted callback still has the old model text. A second tap cannot submit again.
  f.act(tree, 'send', undefined, { text: 'duplicate' });
  await tick();
  assert.equal(f.submissions.length, 1);
  assert.equal(f.submissions[0]!.caption, finalCaption);
  assert.equal(f.submissions[0]!.send, true);
  assert.equal(f.preferred(), null);
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  assert.equal(f.picked(), undefined, 'preparing bytes must not complete the still-mounted sheet');
  f.close(tree);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((value) => value.uri),
    ['file:///owned/b', 'file:///owned/a'],
  );
});

test('caption and Send reject malformed text and foreign, stale or retired presentation callbacks', async () => {
  const f = await fixture(true);
  f.photos.select(item('a'));
  const stale = f.render();
  f.photos.select(item('b'));
  const tree = f.render();
  for (const text of ['x'.repeat(4097), 'nul\0text', 42]) {
    f.act(tree, 'caption', undefined, { text });
    f.act(tree, 'send', undefined, { text });
  }
  for (const payload of [
    { token: 'other' },
    { context: 'other-chat' },
    { revision: 'other-render' },
  ]) {
    f.act(tree, 'caption', undefined, { text: 'foreign', ...payload });
    f.act(tree, 'send', undefined, { text: 'foreign', ...payload });
  }
  f.act(stale, 'send', undefined, { text: 'old selection' });
  assert.equal(JSON.parse(f.render().props.model).caption.value, '');
  f.retire();
  f.act(tree, 'send', undefined, { text: 'retired' });
  await tick();
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
});

test('direct Send admits a 4096-character caption but rejects an empty or busy selection', async () => {
  const f = await fixture(true);
  f.act(f.render(), 'send', undefined, { text: 'nothing selected' });
  assert.deepEqual(f.submissions, []);
  f.photos.select(item('a'));
  f.act(f.render({ ...f.photos.getSnapshot(), busy: true }), 'send', undefined, { text: 'busy' });
  assert.deepEqual(f.submissions, []);
  const tree = f.render();
  f.act(tree, 'send', undefined, { text: 'x'.repeat(4096) });
  await tick();
  assert.equal(f.submissions.length, 1);
  const submission = f.submission();
  assert.ok(submission);
  assert.equal(submission.caption.length, 4096);
  f.close(tree);
  await tick();
});

test('Photos More sends only an admitted file intent with exact ordered IDs and final native caption after export', async () => {
  const f = await fixture(true, undefined, undefined, true);
  assert.deepEqual(JSON.parse(f.render().props.model).sendOptions, []);
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  const tree = f.render(),
    menu = JSON.parse(tree.props.model).sendOptions;
  assert.equal(menu.length, 2);
  assert.equal(menu[0].id, 'send-as-file');
  assert.equal(menu[0].enabled, true);
  const exportReady = deferred<AttachmentAsset[]>();
  f.materializeNext(() => exportReady.promise);
  f.act(tree, 'option', 'send-as-file', {
    ids: ['ph://b', 'ph://a'],
    text: 'Final file caption 🪁',
  });
  assert.deepEqual(f.submissions, [], 'No terminal mode before materialization succeeds');
  exportReady.resolve([
    { uri: 'file:///owned/b', mime: 'image/jpeg', name: 'b.jpg', size: 12 },
    { uri: 'file:///owned/a', mime: 'image/jpeg', name: 'a.jpg', size: 12 },
  ]);
  await tick();
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  assert.equal(f.submission()?.caption, 'Final file caption 🪁');
  assert.equal(f.submission()?.options?.presentation, 'file');
  assert.equal(f.submission()?.options?.grouping, 'separate');
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Final file caption 🪁', '']);
  assert.equal(f.picked(), undefined, 'The actual close barrier still owns handoff');
  f.close(tree);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((file) => file.name),
    ['b.jpg', 'a.jpg'],
  );
});

test('file option rejects unsupported mode, stale selection/revision, malformed caption and disabled admission', async () => {
  const blocked = await fixture(true);
  blocked.photos.select(item('a'));
  const unavailable = blocked.render();
  assert.deepEqual(JSON.parse(unavailable.props.model).sendOptions, []);
  blocked.act(unavailable, 'option', 'send-as-file', { ids: ['ph://a'], text: 'blocked' });
  assert.deepEqual(blocked.copies, []);
  blocked.retire();
  const f = await fixture(true, undefined, undefined, true);
  f.photos.select(item('a'));
  const old = f.render();
  f.photos.select(item('b'));
  const tree = f.render();
  for (const [id, payload] of [
    ['unknown', { ids: ['ph://a', 'ph://b'], text: 'bad option' }],
    ['send-as-file', { ids: ['ph://b', 'ph://a'], text: 'wrong order' }],
    ['send-as-file', { ids: ['ph://a'], text: 'missing member' }],
    ['send-as-file', { ids: ['ph://a', 'ph://b'], text: 'bad\0caption' }],
    ['send-as-file', { ids: ['ph://a', 'ph://b'], text: 'x'.repeat(4097) }],
    ['send-as-file', { ids: ['ph://a', 'ph://b'], text: 'foreign', revision: 'foreign' }],
  ] as const)
    f.act(tree, 'option', id, payload);
  f.act(old, 'option', 'send-as-file', { ids: ['ph://a'], text: 'stale menu' });
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  f.retire();
  f.act(tree, 'option', 'send-as-file', { ids: ['ph://a', 'ph://b'], text: 'retired' });
  assert.deepEqual(f.copies, []);
});

test('failed as-file export leaves no mode for a later ordinary media send', async () => {
  const f = await fixture(true, undefined, undefined, true);
  f.photos.select(item('a'));
  f.materializeNext(async () => {
    throw new Error('export failed');
  });
  f.act(f.render(), 'option', 'send-as-file', { ids: ['ph://a'], text: 'failed file intent' });
  await tick();
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.deepEqual(f.submissions, []);
  f.materializeNext(async () => [
    { uri: 'file:///owned/a', mime: 'image/jpeg', name: 'a.jpg', size: 12 },
  ]);
  const ordinary = f.render();
  f.act(ordinary, 'send', undefined, { text: 'ordinary caption' });
  await tick();
  assert.equal(f.submission()?.options, undefined);
  assert.equal(f.submission()?.caption, 'ordinary caption');
  f.close(ordinary);
  await tick();
});

test('retirement during as-file export cannot record a terminal file submission', async () => {
  const f = await fixture(true, undefined, undefined, true),
    pending = deferred<AttachmentAsset[]>();
  f.photos.select(item('a'));
  f.materializeNext(() => pending.promise);
  f.act(f.render(), 'option', 'send-as-file', { ids: ['ph://a'], text: 'canceled' });
  f.retire();
  pending.resolve([{ uri: 'file:///owned/a', mime: 'image/jpeg', name: 'a.jpg', size: 12 }]);
  await tick();
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.picked(), []);
});

test('removed legacy Review cannot prepare files and native Preview stays inside the same Photos task', async () => {
  const review = await fixture(true);
  review.photos.select(item('a'));
  review.act(review.render(), 'caption', undefined, { text: 'Review this first' });
  const tree = review.render();
  review.act(tree, 'review');
  await tick();
  assert.deepEqual(review.submissions, []);
  assert.deepEqual(review.copies, []);
  assert.equal(review.photos.getSnapshot().visible, true);
  assert.equal(review.picked(), undefined);
  review.retire();
  await tick();

  const preview = await fixture(true);
  preview.photos.select(item('a'));
  const token = preview.photos.getSnapshot().token;
  preview.act(preview.render(), 'preview', 'ph://b');
  await tick();
  assert.equal(preview.photos.getSnapshot().screen, 'preview');
  assert.equal(preview.photos.getSnapshot().previewId, 'ph://b');
  assert.equal(preview.photos.getSnapshot().visible, true);
  assert.equal(preview.photos.getSnapshot().token, token);
  assert.deepEqual(
    preview.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  assert.deepEqual(preview.submissions, []);
  assert.deepEqual(preview.copies, []);
  assert.equal(preview.picked(), undefined);
  preview.act(preview.render(), 'back');
  assert.equal(preview.photos.getSnapshot().screen, 'grid');
  preview.retire();
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
test('native preview navigates without selecting or preparing the viewed item', async () => {
  const f = await fixture();
  f.photos.select(item('b'));
  const tree = f.render();
  f.act(tree, 'preview', 'ph://a');
  await tick();
  assert.deepEqual(f.copies, []);
  assert.equal(f.preferred(), null);
  assert.equal(f.photos.getSnapshot().screen, 'preview');
  assert.equal(f.photos.getSnapshot().previewId, 'ph://a');
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  assert.equal(f.picked(), undefined);
  f.act(f.render(), 'back');
  assert.equal(f.photos.getSnapshot().screen, 'grid');
  assert.equal(f.photos.getSnapshot().previewId, null);
  f.retire();
  await tick();
});
test('retained category selection never closes or copies, and Browse waits native close before the existing Files picker', async () => {
  const f = await fixture(),
    tree = f.render();
  f.act(tree, 'document');
  assert.equal(
    f.keyboard.getSnapshot().presentation?.closing,
    false,
    'Browse cannot be invoked from the Photos child',
  );
  f.act(tree, 'category', 'document');
  assert.equal(f.photos.getSnapshot().category, 'document');
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.equal(f.keyboard.getSnapshot().presentation?.closing, false);
  assert.deepEqual(f.calls, ['photo']);
  assert.deepEqual(f.copies, []);
  assert.equal(f.picked(), undefined);
  const files = f.render();
  assert.equal(JSON.parse(files.props.model).activeCategory, 'document');
  f.act(files, 'document');
  assert.deepEqual(f.calls, ['photo']);
  assert.equal(f.keyboard.getSnapshot().presentation?.closing, false);
  assert.equal(f.photos.getSnapshot().visible, false);
  await f.close(files);
  await tick();
  assert.deepEqual(f.calls, ['photo', 'browse', 'photo-drained']);
  assert.equal(f.keyboard.getSnapshot().presentation, null);
  assert.equal((f.picked() as AttachmentAsset[])[0]?.mime, 'application/pdf');
  const g = await fixture();
  g.photos.select(item('a'));
  const selected = g.render();
  g.act(selected, 'category', 'audio');
  g.act(selected, 'audio');
  assert.equal(g.photos.getSnapshot().category, 'photos');
  assert.equal(g.keyboard.getSnapshot().presentation?.closing, false);
  g.retire();
  await tick();
});
test('cancelled mounted Browse resumes the same Files category and Photos owner with a fresh native token', async () => {
  const f = await fixture();
  f.browseNext(async () => []);
  f.act(f.render(), 'category', 'document');
  const files = f.render(),
    original = f.request.id;
  f.act(files, 'document');
  assert.equal(f.photos.getSnapshot().visible, false);
  assert.deepEqual(f.calls, ['photo']);
  await f.close(files);
  await tick();
  assert.deepEqual(f.calls, ['photo', 'browse']);
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.equal(f.photos.getSnapshot().category, 'document');
  assert.equal(f.keyboard.ownsPicker(original), true);
  const resumed = f.render();
  assert.notEqual(f.request.id, original);
  assert.equal(JSON.parse(resumed.props.model).activeCategory, 'document');
  assert.deepEqual(f.copies, []);
  assert.equal(f.picked(), undefined);
  await f.close(files);
  assert.deepEqual(f.calls, ['photo', 'browse']);
  f.retire();
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
    const f = await fixture();
    if (action === 'document') f.act(f.render(), 'category', 'document');
    const tree = f.render();
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

test('cancelled Camera resumes the existing Photos selection and caption with a fresh native token', async () => {
  const f = await fixture(true);
  f.photos.select(item('b'));
  f.captureNext(async () => []);
  f.act(f.render(), 'caption', undefined, { text: 'Keep the selected photo and caption' });
  const tree = f.render(),
    original = f.request.id,
    photoToken = f.photos.getSnapshot().token;
  f.act(tree, 'camera');
  assert.equal(f.photos.getSnapshot().visible, false);
  await f.close(tree);
  await tick();
  assert.deepEqual(f.captures, [2]);
  assert.equal(f.resumed.length, 1);
  assert.notEqual(f.request.id, original);
  assert.equal(f.keyboard.ownsPicker(original), true);
  assert.equal(f.photos.getSnapshot().token, photoToken);
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  const current = f.render();
  assert.equal(
    JSON.parse(current.props.model).caption.value,
    'Keep the selected photo and caption',
  );
  assert.equal(f.picked(), undefined);
  assert.deepEqual(f.calls, ['photo']);
  // Old native view callbacks cannot address this resumed presentation.
  f.act(tree, 'send', undefined, { text: 'old Send' });
  f.act(tree, 'select', 'ph://a');
  await f.close(tree);
  assert.equal(f.resumed.length, 1);
  assert.equal(f.submissions.length, 1);
  assert.equal(f.submission()?.send, false);
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  f.act(current, 'send', undefined, { text: 'Current final caption' });
  await tick();
  assert.equal(f.submissions.length, 2);
  const submission = f.submission();
  assert.ok(submission);
  assert.equal(submission.caption, 'Current final caption');
  assert.equal(submission.send, true);
  assert.deepEqual(f.copies, [['ph://b']]);
  await f.close(current);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((value) => value.uri),
    ['file:///owned/b'],
  );
  assert.equal(f.keyboard.ownsPicker(original), false);
});

test('duplicate and foreign native close cannot launch Camera twice or rotate a resumed Photos owner twice', async () => {
  const f = await fixture(true),
    cancelled = deferred<AttachmentAsset[]>();
  f.captureNext(() => cancelled.promise);
  const tree = f.render();
  f.act(tree, 'camera');
  await tree.props.onClosed({
    nativeEvent: { token: 'foreign', context: f.request.field.context, reason: 'closed' },
  });
  assert.deepEqual(f.captures, []);
  const firstClose = f.close(tree),
    duplicateClose = f.close(tree);
  await tick();
  assert.deepEqual(f.captures, [3]);
  assert.equal(f.resumed.length, 0);
  cancelled.resolve([]);
  await Promise.all([firstClose, duplicateClose]);
  await tick();
  assert.equal(f.resumed.length, 1);
  await f.close(tree);
  assert.equal(f.resumed.length, 1);
  assert.deepEqual(f.calls, ['photo']);
  f.retire();
  await tick();
});

test('retirement during cancelled Camera capture cannot reopen Photos or revive its initial token', async () => {
  const f = await fixture(true),
    cancelled = deferred<AttachmentAsset[]>();
  f.photos.select(item('a'));
  f.captureNext(() => cancelled.promise);
  const tree = f.render(),
    original = f.request.id;
  f.act(tree, 'camera');
  const closed = f.close(tree);
  await tick();
  assert.deepEqual(f.captures, [2]);
  f.retire();
  cancelled.resolve([]);
  await closed;
  await tick();
  assert.deepEqual(f.resumed, []);
  assert.equal(f.keyboard.ownsPicker(original), false);
  assert.equal(f.keyboard.getSnapshot().presentation, null);
  assert.equal(f.submissions.filter((value) => value.send).length, 0);
  assert.deepEqual(f.picked(), []);
});

test('actual Photos hook subscription retains its initial task owner across a resumed native presentation', async () => {
  type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  const slot = () => (slots[cursor++] ??= {});
  const same = (a: readonly unknown[] | undefined, b: readonly unknown[]) =>
    !!a && a.length === b.length && a.every((value, index) => Object.is(value, b[index]));
  const effect = (read: () => void | (() => void), deps: readonly unknown[]) => {
    const current = slot();
    if (same(current.deps, deps)) return;
    current.deps = deps;
    effects.push(() => {
      current.cleanup?.();
      current.cleanup = read() ?? undefined;
    });
  };
  let token = 0;
  const libraryObserver = { changed: null as (() => void) | null, subscriptions: 0 };
  const keyboard = new AttachmentKeyboardSession('chat', true, () => `hook-${++token}`);
  keyboard.bind({ tag: 7, context: 'chat', label: 'Message', placeholder: 'Message' });
  const jsx = (type: unknown, props: Record<string, any>) => ({ type, props });
  const ports: Record<string, unknown> = {
    './attachment-categories': attachmentCategories,
    './attachment-recent-session': { AttachmentRecentSession, recentAttachmentId },
    '../../modules/zoen-core/src/ZoenCoreModule': { releaseLocalFile: async () => {} },
    react: {
      useState: (initial: unknown) => {
        const current = slot();
        if (!Object.hasOwn(current, 'value')) current.value = initial;
        return [
          current.value,
          (next: unknown) => {
            current.value = typeof next === 'function' ? next(current.value) : next;
          },
        ];
      },
      useRef: (initial: unknown) => {
        const current = slot();
        return (current.value ??= { current: initial });
      },
      useMemo: (read: () => unknown, deps: readonly unknown[]) => {
        const current = slot();
        if (!same(current.deps, deps)) {
          current.value = read();
          current.deps = deps;
        }
        return current.value;
      },
      useEffect: effect,
      useSyncExternalStore: (_subscribe: unknown, read: () => unknown) => read(),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': { AppState: { addEventListener: () => ({ remove() {} }) } },
    'expo-crypto': { randomUUID: () => `hook-render-${++token}` },
    '@expo/ui': {},
    '@expo/ui/swift-ui': {},
    '@expo/ui/swift-ui/modifiers': {},
    'expo-image': {},
    'expo-camera': {},
    'expo-haptics': {},
    '../components/primitives': {},
    '../theme': {},
    '../i18n': {},
    './attachment-photo-session': { AttachmentPhotoSession },
    './attachment-photo-library': {
      photoLibrary: {
        permission: async () => ({ granted: true, status: 'granted', accessPrivileges: 'all' }),
        requestPermission: async () => {
          throw new Error('permission already granted');
        },
        load: async () => [item('a')],
        refresh: async (items: readonly PhotoLibraryItem[]) => [...items],
        albums: async () => [],
        subscribe: (changed: () => void) => {
          libraryObserver.changed = changed;
          libraryObserver.subscriptions++;
          return () => {
            libraryObserver.changed = null;
          };
        },
        manage: async () => {},
        materialize: async () => [],
        discard: () => {},
      },
    },
    './attachment-picker': { pickAttachmentAssets: async () => [] },
    './attachment-image-surface': {},
    './attachment-keyboard-context': { useAttachmentKeyboardSession: () => keyboard },
    './attachment-photo-keyboard.ios': { AttachmentPhotoKeyboard: 'AttachmentPhotoKeyboard' },
  };
  type HookResult = {
    pick(remaining: number, current: () => boolean): Promise<AttachmentAsset[]>;
    view: ReturnType<typeof jsx>;
    submission(): { caption: string; send: boolean } | null;
  };
  const module = { exports: {} as { useAttachmentPhotoPicker(): HookResult } };
  runInNewContext(pickerCode, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const render = () => {
    cursor = 0;
    effects = [];
    const result = module.exports.useAttachmentPhotoPicker();
    effects.forEach((run) => run());
    return result;
  };
  let hook = render();
  assert.equal(libraryObserver.subscriptions, 0, 'Chat mount must not enumerate/register Photos');
  const picked = deferred<AttachmentAsset[]>();
  keyboard.register(async () => picked.resolve(await hook.pick(3, () => true)), true);
  keyboard.openAttachments();
  await tick();
  hook = render();
  assert.equal(hook.view.type, 'AttachmentPhotoKeyboard');
  const initialView = hook.view,
    session = initialView.props.session as AttachmentPhotoSession,
    original = keyboard.getSnapshot().presentation!.id;
  session.select(item('a'));
  assert.equal(libraryObserver.subscriptions, 1);
  libraryObserver.changed?.();
  await tick();
  assert.deepEqual(
    session.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  initialView.props.onSubmission({ caption: 'Retain hook submission', send: false });
  session.camera();
  keyboard.closed(original);
  await session.dismissed(session.getSnapshot().token);
  assert.equal(session.getSnapshot().visible, true);
  const resumed = keyboard.resumePicker(original);
  assert.ok(resumed);
  // resumePicker publishes synchronously. The actual mounted hook's subscription
  // must consult ownsPicker(initialId), not owns(the retired native token).
  assert.equal(session.getSnapshot().visible, true);
  assert.deepEqual(
    session.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  initialView.props.onResume(resumed);
  hook = render();
  assert.equal(hook.view.props.request.id, resumed.id);
  assert.equal(hook.submission()?.caption, 'Retain hook submission');
  keyboard.retire();
  assert.equal(session.getSnapshot().visible, false);
  assert.deepEqual(await picked.promise, []);
  for (const current of slots) current.cleanup?.();
});

test('gallery Send includes its current unselected item and the final native keystroke only after explicit Send', async () => {
  const f = await fixture(true);
  f.act(f.render(), 'preview', 'ph://b');
  const tree = f.render();
  assert.deepEqual(f.photos.getSnapshot().selected, []);
  assert.equal(JSON.parse(tree.props.model).caption.sendEnabled, true);
  assert.equal(JSON.parse(tree.props.model).caption.count, 0);
  assert.equal(JSON.parse(tree.props.model).caption.sendLabel, 'media.attachmentSendCount:1');
  assert.equal(JSON.parse(tree.props.model).labels.edit, 'media.attachmentEdit');
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  f.act(tree, 'send', 'ph://b', { text: 'Final gallery keystroke!' });
  f.act(tree, 'send', 'ph://b', { text: 'Duplicate' });
  await tick();
  assert.equal(f.submissions.length, 1);
  assert.equal(f.submission()?.caption, 'Final gallery keystroke!');
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Final gallery keystroke!']);
  assert.deepEqual(f.copies, [['ph://b']]);
  assert.equal(f.picked(), undefined);
  f.close(tree);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((value) => value.uri),
    ['file:///owned/b'],
  );
});

test('gallery caption and Send reject missing, foreign and noncurrent item identities without changing selection', async () => {
  const f = await fixture(true);
  f.photos.select(item('a'));
  f.act(f.render(), 'preview', 'ph://b');
  const tree = f.render();
  for (const id of ['ph://foreign', 'ph://a']) {
    f.act(tree, 'caption', id, { text: 'Wrong photo' });
    f.act(tree, 'send', id, { text: 'Wrong photo' });
  }
  f.act(tree, 'caption', undefined, { text: 'Missing gallery identity' });
  f.act(tree, 'send', undefined, { text: 'Missing gallery identity' });
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  assert.equal(JSON.parse(f.render().props.model).caption.value, '');
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  f.retire();
  await tick();
});

test('grid Send rejects a supplied gallery identity and an empty grid remains unsendable', async () => {
  const f = await fixture(true);
  assert.equal(JSON.parse(f.render().props.model).caption.sendEnabled, false);
  f.act(f.render(), 'send', undefined, { text: 'No selection' });
  f.photos.select(item('a'));
  f.act(f.render(), 'send', 'ph://foreign', { text: 'Unexpected gallery identity' });
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  f.retire();
  await tick();
});

test('per-photo gallery captions survive navigation and follow the exact reordered selection on Send', async () => {
  const f = await fixture(true);
  f.act(f.render(), 'preview', 'ph://a');
  f.act(f.render(), 'caption', 'ph://a', { text: 'Caption A' });
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  f.act(f.render(), 'preview', 'ph://b');
  assert.equal(JSON.parse(f.render().props.model).caption.value, '');
  f.act(f.render(), 'caption', 'ph://b', { text: 'Caption B before final key' });
  f.act(f.render(), 'preview', 'ph://a');
  assert.equal(JSON.parse(f.render().props.model).caption.value, 'Caption A');
  f.act(f.render(), 'reorder', undefined, { ids: ['ph://b', 'ph://a'] });
  f.act(f.render(), 'preview', 'ph://b');
  assert.equal(JSON.parse(f.render().props.model).caption.value, 'Caption B before final key');
  const tree = f.render();
  f.act(tree, 'send', 'ph://b', { text: 'Caption B before final key!' });
  await tick();
  assert.equal(f.submission()?.caption, 'Caption B before final key!');
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), [
    'Caption B before final key!',
    'Caption A',
  ]);
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  f.close(tree);
  await tick();
});

test('a grid common caption replaces item captions on the first selected asset without leaking across Photos owners', async () => {
  const f = await fixture(true);
  f.act(f.render(), 'preview', 'ph://a');
  f.act(f.render(), 'caption', 'ph://a', { text: 'Old per-photo caption' });
  f.act(f.render(), 'back');
  f.photos.select(item('b'));
  f.act(f.render(), 'caption', undefined, { text: 'Common caption' });
  f.act(f.render(), 'preview', 'ph://b');
  assert.equal(JSON.parse(f.render().props.model).caption.value, 'Common caption');
  const tree = f.render();
  f.act(tree, 'send', 'ph://b', { text: 'Common caption final' });
  await tick();
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Common caption final', '']);
  assert.deepEqual(f.copies, [['ph://a', 'ph://b']]);
  f.close(tree);
  await tick();
  const next = await fixture(true);
  next.act(next.render(), 'preview', 'ph://a');
  assert.equal(JSON.parse(next.render().props.model).caption.value, '');
  next.retire();
  await tick();
});

test('supported Photos video Edit stages the current exact selection for the durable owner without sending', async () => {
  for (const extension of ['mp4', 'mov', 'm4v']) {
    const video = {
      ...item('video'),
      filename: `clip.${extension}`,
      mediaType: 'video' as PhotoLibraryItem['mediaType'],
    };
    const f = await fixture(true, [item('a'), video]);
    f.photos.select(item('a'));
    f.act(f.render(), 'preview', video.id);
    const tree = f.render();
    assert.equal(JSON.parse(tree.props.model).canEdit, true);
    f.act(tree, 'edit', video.id, { text: 'Final video caption' });
    f.act(tree, 'edit', video.id, { text: 'Duplicate' });
    await tick();
    assert.equal(f.submissions.length, 1);
    assert.equal(f.submission()?.send, false);
    assert.equal(f.submission()?.requestEdit, true);
    assert.equal(f.submission()?.caption, '');
    assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['', 'Final video caption']);
    assert.deepEqual(f.copies, [['ph://a', video.id]]);
    assert.equal(f.preferred(), 1);
    assert.equal(f.picked(), undefined, 'Editing must wait for the native presentation to close');
    f.close(tree);
    await tick();
    assert.equal((f.picked() as AttachmentAsset[])[1]?.name, `clip.${extension}`);
    f.retire();
  }
});

test('unsupported Photos video format cannot prepare an edit or fall through to Send', async () => {
  const video = {
    ...item('video'),
    filename: 'clip.webm',
    mediaType: 'video' as PhotoLibraryItem['mediaType'],
  };
  const f = await fixture(true, [video]);
  f.act(f.render(), 'preview', video.id);
  const tree = f.render();
  assert.equal(JSON.parse(tree.props.model).canEdit, false);
  f.act(tree, 'edit', video.id, { text: 'Unsupported' });
  await tick();
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  f.retire();
});

const recentScope = {
  expected_identity_id: 'account',
  expected_device_id: 'device',
  space_id: 'space',
  thread_root_id: null,
};
function recentFixture() {
  const rows = ['a', 'b'].map((id) => ({
    reference: {
      space_id: 'source',
      message_id: id,
      item_id: `file-${id}`,
      version: 1,
      sha256: 'a'.repeat(64),
    },
    name: `${id}.pdf`,
    mime: 'application/pdf',
    bytes: '12',
    at_ms: '100',
    space_title: 'Source',
    author_title: 'Author',
    ready: true,
  }));
  const materialized: string[] = [],
    previews: string[] = [],
    released: string[] = [];
  let fail = false;
  const port = {
    list: async () => ({ items: rows, next_cursor: null }),
    materialize: async (
      _scope: unknown,
      _kind: unknown,
      reference: (typeof rows)[number]['reference'],
    ) => {
      if (fail) throw new Error('Source changed');
      materialized.push(reference.message_id);
      return {
        uri: `file:///native-owned/${reference.message_id}`,
        name: `${reference.message_id}.pdf`,
        mime: 'application/pdf',
        size: 12,
      };
    },
    preview: async (
      _scope: unknown,
      _kind: unknown,
      reference: (typeof rows)[number]['reference'],
    ) => {
      previews.push(reference.message_id);
    },
    release: async (uri: string) => {
      released.push(uri);
    },
  };
  return {
    recent: new AttachmentRecentSession(),
    port,
    rows,
    materialized,
    previews,
    released,
    fail: () => {
      fail = true;
    },
  };
}

test('real File recent projection sends exact owned copies and final caption only after native removal', async () => {
  const r = recentFixture(),
    f = await fixture(true, undefined, r.recent);
  f.photos.showCategory('document');
  r.recent.activate({ scope: recentScope, port: r.port }, 'file', 3, () =>
    f.photos.isCurrent(f.photos.getSnapshot().token),
  );
  await tick();
  const first = f.render(),
    model = JSON.parse(first.props.model);
  assert.deepEqual(
    Array.from(model.fileItems, (row: any) => row.id),
    r.rows.map(recentAttachmentId),
  );
  assert.deepEqual(model.audioItems, []);
  f.act(first, 'recentSelect', recentAttachmentId(r.rows[1]!));
  f.act(f.render(), 'recentSelect', recentAttachmentId(r.rows[0]!));
  const selected = f.render(),
    selectedModel = JSON.parse(selected.props.model);
  assert.equal(selectedModel.caption.count, 2);
  assert.equal(selectedModel.caption.sendEnabled, true);
  f.act(selected, 'document');
  assert.equal(
    f.photos.getSnapshot().visible,
    true,
    'Browse cannot drop existing recent selections',
  );
  f.act(selected, 'recentSend', undefined, { text: 'Final native caption' });
  await tick();
  assert.deepEqual(r.materialized, ['b', 'a']);
  assert.equal(f.picked(), undefined);
  assert.equal(f.submission()?.send, true);
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Final native caption', '']);
  f.close(selected);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((file) => file.uri),
    ['file:///native-owned/b', 'file:///native-owned/a'],
  );
  assert.equal(
    (f.picked() as AttachmentAsset[]).some((file) => 'reference' in file),
    false,
  );
  r.recent.retire();
  f.retire();
});

test('failed recent Send cannot leak its submission intent into an ordinary Browse result', async () => {
  const r = recentFixture(),
    f = await fixture(true, undefined, r.recent);
  f.photos.showCategory('document');
  r.recent.activate({ scope: recentScope, port: r.port }, 'file', 3, () =>
    f.photos.isCurrent(f.photos.getSnapshot().token),
  );
  await tick();
  r.fail();
  f.act(f.render(), 'recentOpen', recentAttachmentId(r.rows[0]!));
  await tick();
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.equal(f.submission(), undefined);
  const browsing = f.render();
  f.act(browsing, 'document');
  assert.equal(f.submission()?.send, false);
  f.close(browsing);
  await tick();
  assert.equal((f.picked() as AttachmentAsset[])[0]?.name, 'document.pdf');
  assert.equal(f.submission()?.send, false);
  r.recent.retire();
  f.retire();
});

test('recent Preview waits for native removal and resumes the same File child without preparing Send', async () => {
  const r = recentFixture(),
    f = await fixture(true, undefined, r.recent);
  f.photos.showCategory('document');
  r.recent.activate({ scope: recentScope, port: r.port }, 'file', 3, () =>
    f.photos.isCurrent(f.photos.getSnapshot().token),
  );
  await tick();
  const tree = f.render();
  f.act(tree, 'recentPreview', recentAttachmentId(r.rows[0]!));
  assert.deepEqual(r.previews, []);
  assert.equal(f.photos.getSnapshot().visible, false);
  f.close(tree);
  await tick();
  assert.deepEqual(r.previews, ['a']);
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.equal(f.photos.getSnapshot().category, 'document');
  assert.equal(f.submission(), undefined);
  assert.deepEqual(r.materialized, []);
  assert.equal(f.picked(), undefined);
  assert.equal(f.resumed.length, 1);
  r.recent.retire();
  f.retire();
});

test('recent state revision rejects an old row action after search activation', async () => {
  const r = recentFixture(),
    f = await fixture(true, undefined, r.recent);
  f.photos.showCategory('document');
  r.recent.activate({ scope: recentScope, port: r.port }, 'file', 3, () =>
    f.photos.isCurrent(f.photos.getSnapshot().token),
  );
  await tick();
  const old = f.render();
  f.act(old, 'recentSearch');
  f.render();
  f.act(old, 'recentOpen', recentAttachmentId(r.rows[0]!));
  await tick();
  assert.deepEqual(r.materialized, []);
  assert.equal(f.submission(), undefined);
  r.recent.retire();
  f.retire();
});

test('Audio Browse always selects ordinary audio files and never falls back to voice recording', async () => {
  const f = await fixture(true);
  f.browseNext(async () => [
    { uri: 'file:///owned/music.m4a', name: 'music.m4a', mime: 'audio/mp4', size: 12 },
  ]);
  f.act(f.render(), 'category', 'audio');
  const audio = f.render();
  f.act(audio, 'audio');
  assert.equal(f.submission()?.send, false);
  assert.equal(f.photos.getSnapshot().visible, false);
  f.close(audio);
  await tick();
  assert.deepEqual(
    f.calls.filter((call) => call === 'audio'),
    [],
  );
  assert.equal(f.calls.includes('browse'), true);
  assert.equal((f.picked() as AttachmentAsset[])[0]?.mime, 'audio/mp4');
  assert.equal('voiceSourceId' in (f.picked() as AttachmentAsset[])[0]!, false);
  f.retire();
});

test('default Photos Send exports a signed-group intent in selection order with the final caption', async () => {
  const f = await fixture(true);
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  const tree = f.render();
  assert.equal(JSON.parse(tree.props.model).grouped, true);
  f.act(tree, 'send', undefined, { text: 'Final album caption' });
  await tick();
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  assert.equal(f.submission()?.options?.grouping, 'grouped');
  assert.equal(f.submission()?.options?.presentation, 'media');
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Final album caption', '']);
  f.close(tree);
  await tick();
  f.retire();
});

test('Send Without Grouping is a typed terminal action with exact selection, revision and close barrier', async () => {
  const f = await fixture(true);
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  const tree = f.render(),
    model = JSON.parse(tree.props.model);
  assert.deepEqual(
    Array.from(model.sendOptions, (option: any) => option.id),
    ['send-without-grouping'],
  );
  f.act(tree, 'option', 'send-without-grouping', {
    ids: ['ph://a', 'ph://b'],
    text: 'wrong order',
  });
  f.act(tree, 'option', 'send-without-grouping', {
    ids: ['ph://b', 'ph://a'],
    text: 'stale',
    revision: 'stale',
  });
  assert.deepEqual(f.copies, []);
  f.act(tree, 'option', 'send-without-grouping', {
    ids: ['ph://b', 'ph://a'],
    text: 'Final separate caption',
  });
  await tick();
  assert.equal(f.submissions.length, 1);
  assert.equal(f.submission()?.options?.grouping, 'separate');
  assert.equal(f.submission()?.options?.presentation, 'media');
  assert.equal(f.submission()?.caption, 'Final separate caption');
  assert.equal(f.picked(), undefined);
  f.close(tree);
  await tick();
  f.retire();
});

test('an existing grouped draft keeps its grouped preview and cannot be partially converted by a picker menu', async () => {
  const f = await fixture(true, undefined, undefined, false, {
    presentation: 'media',
    grouping: 'grouped',
  });
  f.photos.select(item('a'));
  f.photos.select(item('b'));
  const tree = f.render(),
    model = JSON.parse(tree.props.model);
  assert.equal(model.grouped, true);
  assert.deepEqual(model.sendOptions, []);
  f.act(tree, 'option', 'send-without-grouping', { ids: ['ph://a', 'ph://b'], text: 'convert' });
  assert.deepEqual(f.submissions, []);
  f.act(tree, 'send', undefined, { text: 'Keep grouped' });
  await tick();
  assert.equal(f.submission()?.options?.grouping, 'grouped');
  f.close(tree);
  await tick();
  f.retire();
});

test('actual mounted category projection hands Article off only after native removal; foreign category and stale model cannot navigate', async () => {
  const f = await fixture();
  const opened: string[] = [];
  const routes = {
    categories: ['article'] as const,
    open: (category: string, current: () => boolean) => {
      assert.equal(current(), true);
      opened.push(category);
    },
  };
  const initial = f.render(undefined, routes);
  assert.ok(
    JSON.parse(initial.props.model).categories.some(
      (category: { id: string }) => category.id === 'article',
    ),
  );
  f.act(initial, 'category', 'location');
  assert.equal(f.photos.getSnapshot().visible, true);
  f.act(initial, 'category', 'article');
  assert.deepEqual(opened, []);
  assert.equal(f.photos.getSnapshot().visible, false);
  const closing = f.render(undefined, routes);
  f.act(initial, 'category', 'article');
  assert.deepEqual(opened, []);
  f.close(closing);
  await tick();
  assert.deepEqual(opened, ['article']);
  assert.deepEqual(f.picked(), []);
  assert.equal(f.copies.length, 0);
  assert.equal(f.submissions.length, 0);
  f.close(closing);
  await tick();
  assert.deepEqual(opened, ['article']);
});
