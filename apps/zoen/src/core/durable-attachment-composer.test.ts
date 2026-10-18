import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { MediaDraftChannel } from '../media/media-draft-channel.ts';
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
function fixture(initial = empty()) {
  type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  let cursor = 0,
    sequence = 0,
    revision = 1;
  let effects: (() => void)[] = [];
  let durable = initial;
  let failRead = false,
    failMaterialize = false;
  const released: string[] = [],
    discarded: string[] = [],
    writes: string[] = [],
    sent: string[] = [];
  let selection: () => Promise<AttachmentAsset[]> = async () => [];
  let active = true;
  const port: MediaDraftPort = {
    load: async () => {
      if (failRead) throw new Error('Unread journal');
      return durable;
    },
    save: async (_scope, expected, json, assets) => {
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
      return durable;
    },
    materialize: async (_scope, expected, id) => {
      assert.equal(expected, durable.revision);
      if (failMaterialize) throw new Error('Preview unavailable');
      const asset = durable.assets.find((asset) => asset.id === id);
      assert.ok(asset);
      return { ...asset, uri: `file:///cache/fresh-${id}` };
    },
    send: async (_scope, expected) => {
      assert.equal(expected, durable.revision);
      sent.push(durable.content_json);
      durable = { ...empty(), revision: String(revision++) };
      return { entries: [{ id: 'accepted' }] as never, draft: durable };
    },
    release: async (uri) => {
      released.push(uri);
    },
  };
  const channel = new MediaDraftChannel(scope, port, () => active);
  const store = {
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
    'react-native': { View: 'View', Text: 'Text' },
    'expo-router': { useFocusEffect: (read: () => () => void) => effect(read, [read]) },
    'expo-crypto': { randomUUID: () => `asset-${++sequence}` },
    '../components/primitives': { NativeButton: 'NativeButton' },
    '../core/provider': { useClient: () => ({ store }) },
    '../i18n': { t: (key: string) => key },
    '../theme': { useTheme: () => ({ colors: {} }) },
    './attachment-draft': {
      MAX_ATTACHMENT_ITEMS: 10,
      MAX_ATTACHMENT_TOTAL_BYTES: 64 * 1024 * 1024,
      AttachmentDraftFailure: class extends Error {},
    },
    './attachment-draft-strip': { AttachmentDraftStrip: 'AttachmentDraftStrip' },
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
    spaceId: 'chat',
    replyTo: null,
    threadRootId: null,
    thread: false,
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
