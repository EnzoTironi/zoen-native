import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import * as attachmentCategories from '../media/attachment-categories.ts';
import type { AttachmentAsset } from '../media/attachment-draft';
import { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';
import type { PhotoLibraryItem } from '../media/attachment-photo-library';
import { photoSendOptions } from '../media/attachment-photo-options.ts';
import {
  AttachmentPhotoSession,
  type PhotoPickerState,
  type PhotoPickerSubmission,
} from '../media/attachment-photo-session.ts';
import { AttachmentRecentSession, recentAttachmentId } from '../media/attachment-recent-session.ts';
import type { MediaDraftAttachmentOptions } from '../media/media-draft-types';

export const compile = async (name: string) =>
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
export const [panelCode, nativeCode, pickerCode] = await Promise.all([
  compile('../media/attachment-photo-keyboard.ios.tsx'),
  compile('../media/keyboard-panel-native.ios.tsx'),
  compile('../media/attachment-photo-picker.ios.tsx'),
]);
export const tick = async () => {
  for (let i = 0; i < 14; i++) await Promise.resolve();
};
export function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
export const item = (id: string): PhotoLibraryItem =>
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
export async function fixture(
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

export const recentScope = {
  expected_identity_id: 'account',
  expected_device_id: 'device',
  space_id: 'space',
  thread_root_id: null,
};
export function recentFixture() {
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
