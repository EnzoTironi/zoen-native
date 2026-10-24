import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import type { AttachmentApprovalProps } from '../media/attachment-approval.types.ts';
import * as attachmentCategories from '../media/attachment-categories.ts';
import type { AttachmentComposerOptions } from '../media/attachment-composer.tsx';
import type { AttachmentAsset } from '../media/attachment-draft.ts';
import type {
  PhotoPickerAdmission,
  PhotoPickerSubmission,
} from '../media/attachment-photo-session.ts';
import { AttachmentReview } from '../media/attachment-review.ts';
import * as mediaDraftContract from '../media/media-draft-contract.ts';
import { MediaDraftChannel, MediaDraftFailure } from '../media/media-draft-channel.ts';
import type { MediaDraftDto, MediaDraftPort, MediaDraftScope } from '../media/media-draft-types.ts';
import { PlaybackCoordinator } from '../media/playback-owner.ts';

export type Element = { type: unknown; props: Record<string, unknown> };
export type Result = {
  active: boolean;
  busy: boolean;
  canAdd: boolean;
  pick(kind: string): Promise<void>;
  field?: {
    value: string;
    editable: boolean;
    onChange(text: string): void;
    onSend(finalCaption?: string): void;
    scope: { replyTo: string | null; thread: boolean };
  };
  review: Element | null;
};
export const compiled = await build({
  stdin: {
    contents: readFileSync(
      process.env.ZOEN_ATTACHMENT_COMPOSER_SOURCE ??
        new URL('../media/durable-attachment-composer.tsx', import.meta.url),
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
      name: 'native-attachment-hook-ports',
      setup(builder) {
        builder.onResolve(
          {
            filter:
              /^\.\/(?:use-durable-attachment-(?:controller)|durable-attachment-(?:types|view|pick|commands|contract|review)|attachment-gallery-journal)$/,
          },
          ({ path }) => ({
            path: fileURLToPath(
              new URL(
                `../media/${path.slice(2)}.${path.endsWith('-view') ? 'tsx' : 'ts'}`,
                import.meta.url,
              ),
            ),
          }),
        );
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
export const scope = {
  expected_identity_id: 'owner',
  expected_device_id: 'device',
  space_id: 'chat',
  thread_root_id: null,
};
export const empty = (): MediaDraftDto => ({
  revision: '0',
  content_json: 'null',
  assets: [],
  attempted: false,
});
export const tick = async () => {
  for (let index = 0; index < 12; index++) await Promise.resolve();
};
export function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
export function fixture(initial = empty(), owner: MediaDraftScope = scope) {
  type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  let cursor = 0,
    sequence = 0,
    presentation = 0,
    revision = 1;
  let effects: (() => void)[] = [];
  let durable = initial;
  let failRead = false,
    failSave = false,
    failMaterialize = false,
    loseSendAck = false,
    holdCancellation = false;
  let saveGate: Promise<void> | null = null;
  let materializeGate: Promise<void> | null = null;
  const materializations: string[] = [];
  let sendCalls = 0;
  const sendReceipts = new Map<string, { entries: never; draft: MediaDraftDto }>();
  const released: string[] = [],
    nativeReleased: string[] = [],
    discarded: string[] = [],
    writes: string[] = [],
    sent: string[] = [];
  const journalEvents: string[] = [];
  const previews: { revision: string; assetId: string; token: string }[] = [];
  const canceled: string[] = [];
  const previewWaiters = new Map<string, ReturnType<typeof deferred<void>>>();
  const previewStarted = deferred<void>();
  const deviceCleanups = new Set<() => Promise<void>>();
  const sessionCleanups = new Set<() => Promise<void>>();
  const storeListeners = new Set<() => void>();
  let background: ((state: string) => void) | undefined;
  let selection: () => Promise<AttachmentAsset[]> = async () => [];
  let active = true;
  const port: MediaDraftPort = {
    load: async () => {
      if (failRead) throw new Error('Unread journal');
      return durable;
    },
    save: async (_scope, expected, json, assets) => {
      journalEvents.push('save-start');
      if (saveGate) await saveGate;
      if (failSave) throw new Error('Synthetic rejected draft save');
      assert.equal(expected, durable.revision);
      writes.push(json);
      const content = JSON.parse(json);
      const refs: string[] =
        content?.items.map((item: { asset_id: string }) => item.asset_id) ?? [];
      durable = {
        revision: String(revision++),
        content_json: json,
        attempted: false,
        assets: refs.map((id) => {
          const input = assets.find((asset) => asset.id === id);
          const old = durable.assets.find((asset) => asset.id === id);
          assert.ok(input || old);
          return (
            old ?? { id, name: input!.name, mime: input!.mime, size: 12, sha256: 'a'.repeat(64) }
          );
        }),
      };
      journalEvents.push('save-accepted');
      return { accepted_revision: durable.revision, draft: durable };
    },
    materialize: async (_scope, expected, id) => {
      assert.equal(expected, durable.revision);
      materializations.push(id);
      if (materializeGate) await materializeGate;
      if (failMaterialize) throw new Error('Preview unavailable');
      const asset = durable.assets.find((asset) => asset.id === id);
      assert.ok(asset);
      return { ...asset, uri: `file:///cache/fresh-${id}` };
    },
    preview: async (_scope, expected, assetId, token) => {
      assert.equal(expected, durable.revision);
      assert.ok(durable.assets.some((asset) => asset.id === assetId));
      previews.push({ revision: expected, assetId, token });
      const waiter = deferred<void>();
      previewWaiters.set(token, waiter);
      previewStarted.resolve();
      await waiter.promise;
    },
    cancelPreview: async (token) => {
      canceled.push(token);
      if (!holdCancellation) previewWaiters.get(token)?.resolve();
    },
    send: async (_scope, expected) => {
      journalEvents.push('send');
      sendCalls++;
      const receipt = sendReceipts.get(expected);
      if (receipt) return { ...receipt, draft: durable };
      assert.equal(expected, durable.revision);
      sent.push(durable.content_json);
      durable = { ...empty(), revision: String(revision++) };
      const result = { entries: [{ id: 'accepted' }] as never, draft: durable };
      sendReceipts.set(expected, result);
      if (loseSendAck) {
        loseSendAck = false;
        throw new Error('Synthetic lost send acknowledgement');
      }
      return result;
    },
    release: async (uri) => {
      released.push(uri);
    },
  };
  const coordinator = new PlaybackCoordinator();
  const channel = new MediaDraftChannel(owner, port, () => active);
  const store = {
    driver: {
      media: {
        drafts: port,
        releaseLocalFile: async (uri: string) => {
          nativeReleased.push(uri);
        },
      },
    },
    subscribe: (listener: () => void) => {
      storeListeners.add(listener);
      return () => storeListeners.delete(listener);
    },
    registerDeviceCleanup: (cleanup: () => Promise<void>) => {
      deviceCleanups.add(cleanup);
      return () => deviceCleanups.delete(cleanup);
    },
    registerSessionCleanup: (cleanup: () => Promise<void>) => {
      sessionCleanups.add(cleanup);
      return () => sessionCleanups.delete(cleanup);
    },
    mediaDrafts: { isCurrent: (candidate: unknown) => active && candidate === channel },
    refresh: async () => {},
  };
  const same = (left?: readonly unknown[], right?: readonly unknown[]) =>
    !!left &&
    !!right &&
    left.length === right.length &&
    left.every((value, index) => Object.is(value, right[index]));
  const slot = () => (slots[cursor++] ??= {});
  const memo = (read: () => unknown, deps: readonly unknown[]) => {
    const current = slot();
    if (!same(current.deps, deps)) {
      current.deps = deps;
      current.value = read();
    }
    return current.value;
  };
  const effect = (read: () => void | (() => void), deps?: readonly unknown[]) => {
    const current = slot();
    if (same(current.deps, deps)) return;
    current.deps = deps;
    effects.push(() => {
      current.cleanup?.();
      current.cleanup = read() ?? undefined;
    });
  };
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  let preferredPhotoUri: string | null = null;
  let nextPhotoSubmission: PhotoPickerSubmission | null = null;
  let photoSubmission: PhotoPickerSubmission | null = null;
  const photoAdmissions: (PhotoPickerAdmission | undefined)[] = [];
  let nativePhotoCancel: (() => void) | undefined;
  let nativePhotoPick:
    | ((
        remaining: number,
        current: () => boolean,
        admission?: PhotoPickerAdmission,
      ) => Promise<AttachmentAsset[]>)
    | null = null;
  const photoPicker = {
    pick: async (_remaining: number, _current: () => boolean, admission?: PhotoPickerAdmission) => {
      photoAdmissions.push(admission);
      if (nativePhotoPick) return nativePhotoPick(_remaining, _current, admission);
      const intent = nextPhotoSubmission;
      nextPhotoSubmission = null;
      photoSubmission = null;
      const files = await selection();
      photoSubmission = intent;
      return files;
    },
    cancel: () => nativePhotoCancel?.(),
    view: null,
    preferredUri: () => preferredPhotoUri,
    submission: () => photoSubmission,
  };
  const ports: Record<string, unknown> = {
    './attachment-categories': attachmentCategories,
    './media-draft-channel': { MediaDraftFailure },
    './media-draft-contract': mediaDraftContract,
    'expo-media-library': { MediaType: { IMAGE: 'image', VIDEO: 'video' } },
    react: {
      useMemo: memo,
      useCallback: (read: unknown, deps: readonly unknown[]) => memo(() => read, deps),
      useEffect: effect,
      useLayoutEffect: effect,
      useRef: (initial: unknown) => {
        const current = slot();
        current.value ??= { current: initial };
        return current.value;
      },
      useState: (initial: unknown) => {
        const current = slot();
        if (!Object.hasOwn(current, 'value'))
          current.value = typeof initial === 'function' ? initial() : initial;
        return [
          current.value,
          (next: unknown) => {
            current.value = typeof next === 'function' ? next(current.value) : next;
          },
        ];
      },
      useSyncExternalStore: (_subscribe: unknown, read: () => unknown) => read(),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': {
      View: 'View',
      Text: 'Text',
      Keyboard: { dismiss: () => {} },
      AppState: {
        addEventListener: (_event: string, listener: (state: string) => void) => {
          background = listener;
          return {
            remove() {
              if (background === listener) background = undefined;
            },
          };
        },
      },
    },
    'expo-router': { useFocusEffect: (read: () => () => void) => effect(read, [read]) },
    'expo-crypto': { randomUUID: () => `asset-${++sequence}` },
    '../components/primitives': { NativeButton: 'NativeButton' },
    '../core/provider': {
      useClient: () => ({
        store,
        snapshot: { conversations: [{ id: owner.space_id, title: 'Synthetic conversation' }] },
      }),
    },
    '../i18n': { t: (key: string) => key },
    '../theme': { useTheme: () => ({ colors: {} }) },
    './attachment-draft': {
      MAX_ATTACHMENT_ITEMS: 10,
      MAX_ATTACHMENT_TOTAL_BYTES: 64 * 1024 * 1024,
      AttachmentDraftFailure: class extends Error {},
    },
    './attachment-review': {
      AttachmentReview: class extends AttachmentReview {
        constructor(port: ConstructorParameters<typeof AttachmentReview>[0]) {
          super({ ...port, token: () => `presentation-${++presentation}` }, coordinator);
        }
      },
    },
    './attachment-draft-strip': { AttachmentDraftStrip: 'AttachmentDraftStrip' },
    './attachment-approval': {
      AttachmentApproval: 'AttachmentApproval',
      supportsMediaApproval: true,
    },
    './playback-owner': { mediaPlayback: coordinator },
    './attachment-picker': { pickAttachmentAssets: () => selection() },
    './attachment-photo-picker': {
      useAttachmentPhotoPicker: () => photoPicker,
    },
    './cache-files': { discardCacheCopy: (uri: string) => discarded.push(uri) },
    './cache-work': { sensitiveCacheWork: { run: (work: () => unknown) => work() } },
  };
  const module = {
    exports: {} as {
      useDurableAttachmentComposer(
        options: AttachmentComposerOptions,
        channel: MediaDraftChannel,
      ): Result;
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  let options: AttachmentComposerOptions = {
    spaceId: owner.space_id,
    replyTo: null,
    threadRootId: owner.thread_root_id,
    thread: Boolean(owner.thread_root_id),
    disabled: false,
    suspended: false,
  };
  const render = (patch: Partial<AttachmentComposerOptions> = {}) => {
    options = { ...options, ...patch };
    cursor = 0;
    effects = [];
    const result = module.exports.useDurableAttachmentComposer(options, channel);
    for (const commit of effects) commit();
    return result;
  };
  const unmount = () => {
    for (const current of slots) current.cleanup?.();
  };
  return {
    nativePhotoPicker: (pick: NonNullable<typeof nativePhotoPick>, cancel?: () => void) => {
      nativePhotoPick = pick;
      nativePhotoCancel = cancel;
    },
    nativePhotoSubmission: (intent: PhotoPickerSubmission) => {
      photoSubmission = intent;
    },
    preferPhoto: (uri: string | null) => {
      preferredPhotoUri = uri;
    },
    channel,
    materializations,
    gateMaterialize: (gate: Promise<void>) => {
      materializeGate = gate;
    },
    render,
    unmount,
    released,
    nativeReleased,
    discarded,
    writes,
    sent,
    journalEvents,
    photoAdmissions,
    previews,
    canceled,
    previewStarted: previewStarted.promise,
    closePreview: () => previewWaiters.get(previews.at(-1)!.token)?.resolve(),
    approval: () =>
      maybeFind(render().review, 'attachment-approval')?.props as
        | AttachmentApprovalProps
        | undefined,
    openNativePreview: (id: string) => {
      const strip = findStrip(render().review);
      (strip.props.onPreview as (id: string) => void)(id);
      const approval = maybeFind(render().review, 'attachment-approval');
      if (approval) (approval.props.onEdit as () => void)();
    },
    enableNativeEditing: () => {
      holdCancellation = true;
      port.edit = async (_scope, expected, assetId, token) => {
        assert.equal(expected, durable.revision);
        assert.ok(durable.assets.some((asset) => asset.id === assetId));
        previews.push({ revision: expected, assetId, token });
        const waiter = deferred<void>();
        previewWaiters.set(token, waiter);
        previewStarted.resolve();
        await waiter.promise;
        return durable;
      };
    },
    saveEditedCopy: () => {
      const selected = previews.at(-1)!;
      const content = JSON.parse(durable.content_json);
      const old = durable.assets.find((asset) => asset.id === selected.assetId)!;
      const id = `edited-${old.id}`;
      content.items = content.items.map((item: { asset_id: string; mutation_id: string }) =>
        item.asset_id === old.id ? { ...item, asset_id: id, mutation_id: `post-${id}` } : item,
      );
      durable = {
        ...durable,
        revision: String(revision++),
        content_json: JSON.stringify(content),
        assets: durable.assets.map((asset) =>
          asset === old ? { ...asset, id, size: 24, sha256: 'b'.repeat(64) } : asset,
        ),
      };
      previewWaiters.get(selected.token)?.resolve();
    },
    holdSaves: (gate: Promise<void>) => {
      saveGate = gate;
    },
    loseNextSendAck: () => {
      loseSendAck = true;
    },
    sendCalls: () => sendCalls,
    playPreview: () => {
      let playing = true;
      const playback = coordinator.register(
        () => {
          playing = false;
        },
        () => playing,
      );
      playback.begin();
      return { isPlaying: () => playing, dispose: () => playback.dispose() };
    },
    background: () => background?.('background'),
    erase: () => Promise.all([...deviceCleanups].map((cleanup) => cleanup())),
    endSession: () => Promise.all([...sessionCleanups].map((cleanup) => cleanup())),
    durable: () => durable,
    select: (assets: AttachmentAsset[] | (() => Promise<AttachmentAsset[]>)) => {
      selection = typeof assets === 'function' ? assets : async () => assets;
    },
    submitPhotos: (intent: PhotoPickerSubmission) => {
      nextPhotoSubmission = intent;
    },
    saveFailure: (value: boolean) => {
      failSave = value;
    },
    readFailure: (value: boolean) => {
      failRead = value;
    },
    previewFailure: (value: boolean) => {
      failMaterialize = value;
    },
    retire: () => {
      active = false;
      for (const listener of storeListeners) listener();
    },
  };
}
export function find(root: unknown, id: string): Element {
  if (Array.isArray(root)) {
    for (const child of root) {
      const value = maybeFind(child, id);
      if (value) return value;
    }
  }
  const found = maybeFind(root, id);
  assert.ok(found, id);
  return found;
}
export function maybeFind(root: unknown, id: string): Element | undefined {
  if (Array.isArray(root)) return root.map((child) => maybeFind(child, id)).find(Boolean);
  if (!root || typeof root !== 'object' || !('props' in root)) return;
  const element = root as Element;
  return element.props.testID === id ? element : maybeFind(element.props.children, id);
}
export const photo = (name = 'photo.jpg'): AttachmentAsset => ({
  uri: `file:///cache/${name}`,
  name,
  mime: 'image/jpeg',
  size: 12,
});

export function findStrip(root: unknown): Element {
  if (Array.isArray(root)) {
    for (const child of root) {
      try {
        return findStrip(child);
      } catch {}
    }
  }
  if (root && typeof root === 'object' && 'props' in root) {
    const element = root as Element;
    if (element.type === 'AttachmentDraftStrip') return element;
    return findStrip(element.props.children);
  }
  throw new Error('AttachmentDraftStrip missing');
}
