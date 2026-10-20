import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { MediaDraftChannel } from '../media/media-draft-channel.ts';
import { AttachmentReview } from '../media/attachment-review.ts';
import { PlaybackCoordinator } from '../media/playback-owner.ts';
import type { AttachmentApprovalProps } from '../media/attachment-approval.types.ts';
import type { MediaDraftScope } from '../media/media-draft-types.ts';
import type { AttachmentComposerOptions } from '../media/attachment-composer.tsx';
import type { AttachmentAsset } from '../media/attachment-draft.ts';
import type { MediaDraftDto, MediaDraftPort } from '../media/media-draft-types.ts';

type Element = { type: unknown; props: Record<string, unknown> };
type Result = {
  active: boolean;
  busy: boolean;
  canAdd: boolean;
  pick(kind: string): Promise<void>;
  field?: {
    value: string;
    editable: boolean;
    onChange(text: string): void;
    onSend(): void;
    scope: { replyTo: string | null; thread: boolean };
  };
  review: Element | null;
};
const compiled = await build({
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
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
const scope = {
  expected_identity_id: 'owner',
  expected_device_id: 'device',
  space_id: 'chat',
  thread_root_id: null,
};
const empty = (): MediaDraftDto => ({
  revision: '0',
  content_json: 'null',
  assets: [],
  attempted: false,
});
const tick = async () => {
  for (let index = 0; index < 12; index++) await Promise.resolve();
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
function fixture(initial = empty(), owner: MediaDraftScope = scope) {
  type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  let cursor = 0,
    sequence = 0,
    presentation = 0,
    revision = 1;
  let effects: (() => void)[] = [];
  let durable = initial;
  let failRead = false,
    failMaterialize = false,
    loseSendAck = false,
    holdCancellation = false;
  let saveGate: Promise<void> | null = null;
  let sendCalls = 0;
  const sendReceipts = new Map<string, { entries: never; draft: MediaDraftDto }>();
  const released: string[] = [],
    discarded: string[] = [],
    writes: string[] = [],
    sent: string[] = [];
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
      if (saveGate) await saveGate;
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
      return { accepted_revision: durable.revision, draft: durable };
    },
    materialize: async (_scope, expected, id) => {
      assert.equal(expected, durable.revision);
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
    driver: { media: { drafts: port } },
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
  const ports: Record<string, unknown> = {
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
    channel,
    render,
    unmount,
    released,
    discarded,
    writes,
    sent,
    previews,
    canceled,
    previewStarted: previewStarted.promise,
    closePreview: () => previewWaiters.get(previews.at(-1)!.token)?.resolve(),
    approval: () =>
      maybeFind(render().review, 'attachment-approval')?.props as
        | AttachmentApprovalProps
        | undefined,
    openNativePreview: (id: string) => {
      const strip = (render().review!.props.children as Element[])[0]!;
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
function find(root: unknown, id: string): Element {
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
function maybeFind(root: unknown, id: string): Element | undefined {
  if (Array.isArray(root)) return root.map((child) => maybeFind(child, id)).find(Boolean);
  if (!root || typeof root !== 'object' || !('props' in root)) return;
  const element = root as Element;
  return element.props.testID === id ? element : maybeFind(element.props.children, id);
}
const photo = (name = 'photo.jpg'): AttachmentAsset => ({
  uri: `file:///cache/${name}`,
  name,
  mime: 'image/jpeg',
  size: 12,
});

test('compiled attachment hook retains batch order, each caption, and the original null reply when the ordinary draft changes', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('one.jpg'), photo('two.jpg')]);
  await f.render().pick('photo');
  let ui = f.render({ replyTo: 'later-reply' });
  assert.equal(ui.field?.scope.replyTo, null);
  ui.field!.onChange('First caption');
  await f.channel.flush();
  ui = f.render();
  assert.equal(ui.field?.value, 'First caption');
  const strip = (ui.review!.props.children as Element[])[0]!;
  (strip.props.onSelect as (id: string) => void)('asset-2');
  ui = f.render();
  ui.field!.onChange('Second caption');
  await f.channel.flush();
  const content = JSON.parse(f.durable().content_json);
  assert.deepEqual(
    content.items.map((item: { caption: string }) => item.caption),
    ['First caption', 'Second caption'],
  );
  ui = f.render();
  ui.field!.onSend();
  await tick();
  assert.equal(f.sent.length, 1);
  assert.equal(JSON.parse(f.sent[0]!).reply_to, null);
  assert.equal(f.render().active, false);
  await f.channel.flush();
  assert.deepEqual(f.discarded.sort(), [photo('one.jpg').uri, photo('two.jpg').uri]);
});

test('compiled attachment hook restores encrypted metadata through fresh preview leases and retries a failed materialization', async () => {
  const content = {
    kind: 'attachments',
    reply_to: 'saved-reply',
    items: [{ asset_id: 'old', caption: 'Still here', mutation_id: 'old' }],
  };
  const f = fixture({
    revision: 'saved',
    content_json: JSON.stringify(content),
    attempted: false,
    assets: [{ id: 'old', name: 'old.jpg', mime: 'image/jpeg', size: 12, sha256: 'a'.repeat(64) }],
  });
  f.previewFailure(true);
  f.render();
  await tick();
  f.render();
  await tick();
  let ui = f.render();
  assert.equal(ui.active, true);
  assert.equal(ui.field?.editable, false);
  f.previewFailure(false);
  (find(ui.review, 'retry-media-draft').props.onPress as () => void)();
  await tick();
  f.render();
  await tick();
  ui = f.render();
  assert.equal(ui.field?.editable, true);
  assert.equal(ui.field?.value, 'Still here');
  assert.equal(ui.field?.scope.replyTo, 'saved-reply');
  f.unmount();
  await f.channel.flush();
  assert.deepEqual(f.released, ['file:///cache/fresh-old']);
  assert.equal(JSON.parse(f.durable().content_json).items[0].caption, 'Still here');
});

test('picker completion after route unmount discards only its owned copies and creates no draft', async () => {
  const f = fixture();
  f.render();
  await tick();
  const wait = deferred<AttachmentAsset[]>();
  f.select(() => wait.promise);
  const picking = f.render().pick('photo');
  f.unmount();
  wait.resolve([photo()]);
  await picking;
  await f.channel.flush();
  assert.deepEqual(f.discarded, [photo().uri]);
  assert.equal(f.writes.length, 0);
});

test('an unread durable draft keeps attachment admission closed until an explicit successful retry', async () => {
  const f = fixture();
  f.readFailure(true);
  f.render();
  await tick();
  let ui = f.render();
  assert.equal(ui.canAdd, false);
  f.select([photo()]);
  await ui.pick('photo');
  assert.equal(f.writes.length, 0);
  f.readFailure(false);
  (find(ui.review, 'retry-media-draft').props.onPress as () => void)();
  await tick();
  ui = f.render();
  assert.equal(ui.canAdd, true);
  await ui.pick('photo');
  assert.equal(f.writes.length, 1);
});

test('a previously rendered Remove action preserves the latest caption before React commits', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('one.jpg'), photo('two.jpg')]);
  await f.render().pick('photo');
  const ui = f.render();
  const strip = (ui.review!.props.children as Element[])[0]!;
  ui.field!.onChange('Latest caption before commit');
  (strip.props.onRemove as (id: string) => void)('asset-2');
  await tick();
  await f.channel.flush();
  const content = JSON.parse(f.durable().content_json);
  assert.equal(content.items.length, 1);
  assert.equal(content.items[0].caption, 'Latest caption before commit');
});

test(
  'pre-send review opens the selected saved asset and preserves captions and inline destination',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo('one.jpg'), { ...photo('report.pdf'), mime: 'application/pdf' }]);
    await f.render({ replyTo: 'inline-root' }).pick('document');
    let ui = f.render({ replyTo: 'later-root' });
    ui.field!.onChange('  Caption 🗂️\nkept exactly  ');
    await f.channel.flush();
    ui = f.render();
    const strip = (ui.review!.props.children as Element[])[0]!;
    (strip.props.onSelect as (id: string) => void)('asset-2');
    ui = f.render();
    ui.field!.onChange('PDF caption');
    const second = (ui.review!.props.children as Element[])[0]!;
    (second.props.onPreview as (id: string) => void)('asset-2');
    await f.previewStarted;
    assert.equal(f.previews.length, 1);
    assert.equal(f.previews[0]!.assetId, 'asset-2');
    assert.equal(f.previews[0]!.revision, f.durable().revision);
    assert.equal(f.render().busy, true);
    const content = JSON.parse(f.durable().content_json);
    assert.equal(content.reply_to, 'inline-root');
    assert.deepEqual(
      content.items.map((item: { caption: string }) => item.caption),
      ['  Caption 🗂️\nkept exactly  ', 'PDF caption'],
    );
    assert.deepEqual(f.sent, [], 'review must never send');
    f.closePreview();
    await tick();
    ui = f.render();
    assert.equal(ui.busy, false);
    assert.equal(ui.field?.value, 'PDF caption');
    assert.equal(ui.field?.scope.replyTo, 'inline-root');
  },
);

test('backgrounding, destination replacement and session revocation cancel review and preserve the durable draft', async () => {
  for (const revoke of ['background', 'retire', 'destination'] as const) {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    f.render();
    f.openNativePreview('asset-1');
    await tick();
    if (revoke === 'destination') f.render({ spaceId: 'another-chat' });
    else f[revoke]();
    await tick();
    assert.deepEqual(f.canceled, ['presentation-1']);
    assert.equal(JSON.parse(f.durable().content_json).items[0].asset_id, 'asset-1');
    assert.deepEqual(f.sent, []);
  }
});

test('a stale Remove action dismisses review before dropping the selected draft asset', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('one.jpg'), photo('two.jpg')]);
  await f.render().pick('photo');
  const strip = (f.render().review!.props.children as Element[])[0]!;
  f.openNativePreview('asset-1');
  await tick();
  (strip.props.onRemove as (id: string) => void)('asset-1');
  await tick();
  await f.channel.flush();
  f.render();
  await f.channel.flush();
  assert.deepEqual(f.canceled, ['presentation-1']);
  assert.deepEqual(
    JSON.parse(f.durable().content_json).items.map((item: { asset_id: string }) => item.asset_id),
    ['asset-2'],
  );
  assert.deepEqual(f.discarded, [photo('one.jpg').uri]);
  assert.deepEqual(f.sent, []);
});

test('unmount and device cleanup coalesce cancellation and preserve the encrypted selection', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo()]);
  await f.render().pick('photo');
  f.openNativePreview('asset-1');
  await tick();
  f.unmount();
  await f.erase();
  await f.channel.flush();
  assert.deepEqual(f.canceled, ['presentation-1']);
  assert.equal(JSON.parse(f.durable().content_json).items[0].asset_id, 'asset-1');
  assert.deepEqual(f.discarded, [photo().uri]);
});

test(
  'a stale Send callback during native edited-copy review cannot dismiss it or send the later edited draft',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.enableNativeEditing();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render({ replyTo: 'reviewed-reply' }).pick('photo');
    let ui = f.render();
    ui.field!.onChange('  Reviewed caption 🪁\nexact bytes  ');
    await f.channel.flush();
    ui = f.render();
    const staleSend = ui.field!.onSend;
    f.openNativePreview('asset-1');
    await f.previewStarted;
    assert.equal(f.channel.getSnapshot().reviewing, true);
    staleSend();
    await tick();
    assert.deepEqual(f.canceled, [], 'Send must not request native dismissal');
    assert.equal(f.sendCalls(), 0, 'Send must not be queued behind the pending editor');
    assert.equal(f.channel.getSnapshot().reviewing, true);
    f.saveEditedCopy();
    await tick();
    f.render();
    await tick();
    ui = f.render();
    assert.equal(ui.busy, false);
    assert.equal(f.sendCalls(), 0, 'Native Save must not awaken the stale Send callback');
    const edited = JSON.parse(f.durable().content_json);
    assert.equal(edited.items[0].asset_id, 'edited-asset-1');
    assert.equal(edited.items[0].mutation_id, 'post-edited-asset-1');
    assert.equal(edited.items[0].caption, '  Reviewed caption 🪁\nexact bytes  ');
    assert.equal(edited.reply_to, 'reviewed-reply');
    assert.equal(f.channel.getSnapshot().value!.assets[0]!.sha256, 'b'.repeat(64));
    staleSend();
    await tick();
    assert.equal(
      f.sendCalls(),
      0,
      'The old callback cannot target the replacement after dismissal',
    );
    ui.field!.onSend();
    await tick();
    assert.equal(f.sendCalls(), 1);
    assert.deepEqual(JSON.parse(f.sent[0]!), edited);
    assert.equal(f.render().active, false);
  },
);

test(
  'a stale Send callback during read-only native review leaves the viewer and draft untouched',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.render();
    await tick();
    f.select([{ ...photo('report.pdf'), mime: 'application/pdf' }]);
    await f.render().pick('document');
    const ui = f.render(),
      staleSend = ui.field!.onSend;
    f.openNativePreview('asset-1');
    await f.previewStarted;
    staleSend();
    await tick();
    assert.deepEqual(f.canceled, []);
    assert.equal(f.sendCalls(), 0);
    f.closePreview();
    await tick();
    assert.equal(f.sendCalls(), 0);
    f.render().field!.onSend();
    await tick();
    assert.equal(f.sendCalls(), 1);
  },
);

test(
  'an explicit Send still drains a legitimately queued caption save before sealing the draft',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const gate = deferred<void>();
    f.holdSaves(gate.promise);
    const ui = f.render();
    ui.field!.onChange('Queued caption 🪁');
    ui.field!.onSend();
    await tick();
    assert.equal(f.sendCalls(), 0);
    gate.resolve();
    await f.channel.flush();
    assert.equal(f.sendCalls(), 1);
    assert.equal(JSON.parse(f.sent[0]!).items[0].caption, 'Queued caption 🪁');
  },
);

test(
  'explicit retry after an ambiguous native Send uses the same receipt without duplicating its message',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    f.loseNextSendAck();
    f.render().field!.onSend();
    await tick();
    assert.equal(f.sendCalls(), 1);
    assert.equal(f.sent.length, 1);
    assert.equal(f.channel.getSnapshot().frozen, true);
    const retry = f.approval()!;
    assert.equal(retry.field.retry, true, 'Ambiguous Send still offers its exact receipt retry');
    assert.equal(retry.state.items[0]!.attempted, true);
    retry.field.onSend();
    await tick();
    assert.equal(f.sendCalls(), 2);
    assert.equal(f.sent.length, 1);
    assert.equal(f.render().active, false);
  },
);

test('full-screen approval opens after media selection, preserves each caption and captured thread target, and Close keeps the draft', async () => {
  const f = fixture(empty(), { ...scope, thread_root_id: 'thread-root' });
  f.render();
  await tick();
  f.select([photo('one.jpg'), { ...photo('two.mov'), mime: 'video/quicktime' }]);
  await f.render({ replyTo: 'captured-reply' }).pick('photo');
  let approval = f.approval()!;
  assert.ok(approval);
  assert.equal(approval.recipient, 'Synthetic conversation');
  assert.equal(approval.field.scope.replyTo, 'captured-reply');
  assert.equal(approval.field.scope.thread, true);
  assert.equal(f.previews.length, 0, 'Approval does not automatically open the native editor');
  approval.field.onChange('Photo caption');
  await f.channel.flush();
  f.render({ replyTo: 'later-reply' });
  approval = f.approval()!;
  approval.onSelect('asset-2');
  approval = f.approval()!;
  approval.field.onChange('Video caption');
  await f.channel.flush();
  assert.equal(approval.field.scope.replyTo, 'captured-reply');
  const staleSend = approval.field.onSend;
  approval.onClose();
  await tick();
  assert.equal(f.approval(), undefined);
  assert.deepEqual(
    JSON.parse(f.durable().content_json).items.map((item: { caption: string }) => item.caption),
    ['Photo caption', 'Video caption'],
  );
  staleSend();
  await tick();
  assert.equal(
    f.sendCalls(),
    0,
    'A dismissed presentation cannot send through its retained callback',
  );
  f.openNativePreview('asset-2');
  await f.previewStarted;
  f.closePreview();
  await tick();
  f.approval()!.field.onSend();
  await tick();
  assert.equal(f.sendCalls(), 1);
  assert.equal(JSON.parse(f.sent[0]!).reply_to, 'captured-reply');
});

test('recovered attachments remain compact until explicitly expanded and expanding never stages new content', async () => {
  const f = fixture({
    revision: 'saved',
    content_json: JSON.stringify({
      kind: 'attachments',
      reply_to: null,
      items: [{ asset_id: 'old', caption: 'Recovered', mutation_id: 'old-post' }],
    }),
    assets: [{ id: 'old', name: 'old.jpg', mime: 'image/jpeg', size: 12, sha256: 'a'.repeat(64) }],
    attempted: false,
  });
  f.render();
  await tick();
  f.render();
  await tick();
  assert.equal(f.approval(), undefined);
  const strip = (f.render().review!.props.children as Element[])[0]!;
  (strip.props.onPreview as (id: string) => void)('old-post');
  const approval = f.approval()!;
  assert.equal(approval.field.value, 'Recovered');
  assert.equal(approval.state.items[0]!.uri, 'file:///cache/fresh-old');
  assert.equal(f.writes.length, 0);
  assert.equal(f.sendCalls(), 0);
  approval.onClose();
  await tick();
  assert.equal(f.approval(), undefined);
  f.unmount();
  await f.channel.flush();
  assert.deepEqual(f.released, ['file:///cache/fresh-old']);
});

test('closing approval drains the pending native child, preserves an already accepted edit, and fences its old Send', async () => {
  const f = fixture();
  f.enableNativeEditing();
  f.render();
  await tick();
  f.select([photo()]);
  await f.render().pick('photo');
  let approval = f.approval()!;
  const staleSend = approval.field.onSend;
  approval.onEdit();
  await f.previewStarted;
  approval.onClose();
  await tick();
  approval = f.approval()!;
  assert.ok(approval, 'Parent presentation stays mounted until its native child drains');
  assert.equal(approval.busy, true);
  assert.deepEqual(f.canceled, ['presentation-1']);
  staleSend();
  f.saveEditedCopy();
  await tick();
  f.render();
  await tick();
  assert.equal(f.approval(), undefined);
  assert.equal(JSON.parse(f.durable().content_json).items[0].asset_id, 'edited-asset-1');
  staleSend();
  await tick();
  assert.equal(f.sendCalls(), 0);
});

test('background, revocation and destination replacement revoke approval callbacks while retaining durable content', async () => {
  for (const retire of ['background', 'retire', 'destination'] as const) {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const old = f.approval()!;
    assert.ok(old);
    if (retire === 'destination') f.render({ spaceId: 'another-chat' });
    else f[retire]();
    await tick();
    assert.equal(f.approval(), undefined);
    old.field.onSend();
    old.onEdit();
    old.onAdd('photo');
    await tick();
    assert.equal(f.sendCalls(), 0);
    assert.equal(f.previews.length, 0);
    assert.equal(f.writes.length, 1);
    assert.equal(JSON.parse(f.durable().content_json).items.length, 1);
    f.unmount();
    await f.channel.retire(false);
  }
});

test('approval Close and session retirement pause native preview playback before React can unmount it', async () => {
  for (const close of ['collapse', 'session', 'background'] as const) {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const approval = f.approval()!;
    const playback = f.playPreview();
    assert.equal(playback.isPlaying(), true);
    assert.equal(approval.isCurrent(), true);
    if (close === 'collapse') approval.onClose();
    else if (close === 'background') f.background();
    else await f.endSession();
    assert.equal(
      playback.isPlaying(),
      false,
      'Native playback must be fenced before any new React commit',
    );
    assert.equal(
      approval.isCurrent(),
      false,
      'Late native playback events cannot reclaim the retired approval',
    );
    await tick();
    assert.equal(f.approval(), undefined);
    assert.equal(f.sendCalls(), 0);
    playback.dispose();
    f.unmount();
    await f.channel.retire(false);
  }
});

test(
  'native edited-copy review freezes input without claiming an attempted Send or retry',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.enableNativeEditing();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const approval = f.approval()!;
    assert.equal(approval.field.retry, false);
    approval.onEdit();
    await f.previewStarted;
    try {
      const reviewing = f.approval()!;
      assert.equal(f.channel.getSnapshot().reviewing, true);
      assert.equal(
        f.channel.getSnapshot().frozen,
        true,
        'Native review must retain the writer barrier',
      );
      assert.equal(reviewing.editing, true);
      assert.equal(reviewing.field.retry, false, 'Editing is not an ambiguous Send');
      assert.equal(
        reviewing.state.items[0]!.attempted,
        false,
        'The strip must not claim an attempted Send',
      );
      assert.equal(reviewing.field.editable, false);
      assert.equal(reviewing.field.sending, true);
      assert.equal(f.sendCalls(), 0);
    } finally {
      f.saveEditedCopy();
      await tick();
      f.render();
      await tick();
      f.unmount();
      await f.channel.retire(false);
    }
  },
);

test(
  'normal accepted native edit keeps the existing approval while privacy retirement never reopens it',
  { timeout: 4000 },
  async () => {
    for (const retirement of [null, 'close', 'background', 'retire', 'destination'] as const) {
      const f = fixture();
      f.enableNativeEditing();
      f.render();
      await tick();
      f.select([photo()]);
      await f.render({ replyTo: 'captured-reply' }).pick('photo');
      let approval = f.approval()!;
      approval.field.onChange('Keep this caption');
      await f.channel.flush();
      approval = f.approval()!;
      const staleSend = approval.field.onSend;
      approval.onEdit();
      await f.previewStarted;
      if (retirement === 'close') approval.onClose();
      else if (retirement === 'destination') f.render({ spaceId: 'other-chat' });
      else if (retirement) f[retirement]();
      await tick();
      f.saveEditedCopy();
      await tick();
      f.render();
      await tick();
      approval = f.approval()!;
      if (retirement)
        assert.equal(approval, undefined, 'An accepted edit cannot restore retired UI');
      else {
        assert.ok(approval, 'Ordinary editor completion retains the original approval');
        assert.equal(approval.field.value, 'Keep this caption');
        assert.equal(approval.field.scope.replyTo, 'captured-reply');
        assert.equal(approval.selectedId, 'post-edited-asset-1');
        assert.equal(approval.state.items[0]!.uri, 'file:///cache/fresh-edited-asset-1');
        assert.equal(approval.field.editable, true);
        assert.equal(approval.field.retry, false);
        assert.equal(approval.busy, false);
      }
      const saved = JSON.parse(f.durable().content_json);
      assert.equal(saved.items[0].caption, 'Keep this caption');
      assert.equal(saved.reply_to, 'captured-reply');
      staleSend();
      await tick();
      assert.equal(
        f.sendCalls(),
        0,
        'Old Send cannot target replacement bytes or retired approval',
      );
      f.unmount();
      await f.channel.retire(false);
    }
  },
);
