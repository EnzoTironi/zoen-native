import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { albumDelivery, type MediaAlbum } from './media-albums.ts';
import { messageCard } from './message-presentation.ts';
import { messageText } from './presentation.ts';
import type { TimelineEntry } from './types';
import type { DecodedMessageImage } from '../media/inline-image-frame';
import type { MosaicLayout } from '../media/mosaic-layout';
type Element = { type: unknown; props: Record<string, any> };
const code = (
  await build({
    stdin: {
      contents: readFileSync(
        new URL('../components/conversation-album-message.tsx', import.meta.url),
        'utf8',
      ),
      loader: 'tsx',
    },
    bundle: false,
    format: 'cjs',
    jsx: 'automatic',
    write: false,
  })
).outputFiles[0]!.text;
function album(): MediaAlbum {
  return {
    id: 'scoped-signed-album',
    spaceId: 'qa',
    presentation: 'media',
    entries: [0, 1].map(
      (index) =>
        ({
          id: `member-${index}`,
          kind: {
            Message: {
              text: index ? 'Second caption' : 'First caption',
              card: { kind_id: 'file', item_id: `file-${index}`, version: 1 },
            },
          },
          author: { id: 'sender', is_me: true },
          delivery: index ? 'Sent' : 'Read',
        }) as TimelineEntry,
    ),
  };
}
function fixture() {
  let cursor = 0,
    writes = 0;
  const hooks: any[] = [],
    cleanups: (() => void)[] = [],
    geometryInputs: unknown[] = [];
  let layout: MosaicLayout | null = {
    width: 201,
    height: 100,
    items: [
      { x: 0, y: 0, width: 100, height: 100, position: 1 },
      { x: 101, y: 0, width: 100, height: 100, position: 2 },
    ],
  };
  const ports: Record<string, unknown> = {
    react: {
      useRef: (initial: unknown) => (hooks[cursor++] ??= { current: initial }),
      useLayoutEffect: (read: () => () => void) => cleanups.push(read()),
      useState: (initial: unknown) => {
        const slot = cursor++;
        if (!(slot in hooks)) hooks[slot] = initial;
        return [
          hooks[slot],
          (next: (old: unknown) => unknown) => {
            hooks[slot] = next(hooks[slot]);
            writes++;
          },
        ];
      },
    },
    'react/jsx-runtime': {
      jsx: (type: unknown, props: any) => ({ type, props }),
      jsxs: (type: unknown, props: any) => ({ type, props }),
    },
    'react-native': { View: 'View', useWindowDimensions: () => ({ width: 390, scale: 3 }) },
    '../core/media-albums': { albumDelivery },
    '../core/message-presentation': { messageCard },
    '../core/presentation': { messageText },
    '../media/mosaic-layout': {
      mediaMosaicLayout: (_width: number, _scale: number, sizes: unknown) => {
        geometryInputs.push(sizes);
        return layout;
      },
    },
    '../theme': {
      useTheme: () => ({
        presentation: { chat: { outgoing: { fill: '#abc' }, incoming: { fill: '#def' } } },
      }),
    },
    './conversation-message': { ConversationMessage: 'CanonicalConversationMessage' },
  };
  const module = { exports: {} as { ConversationAlbumMessage(props: any): Element | Element[] } };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const callbacks = {
    onReply() {},
    onThread() {},
    onMenu() {},
    onQuote() {},
    onReact() {},
    onAuthor() {},
  };
  return {
    render: (value: MediaAlbum) => {
      cursor = 0;
      return module.exports.ConversationAlbumMessage({
        ...callbacks,
        album: value,
        spaceId: value.spaceId,
        entry: value.entries[0],
      }) as Element;
    },
    callbacks,
    geometryInputs,
    writes: () => writes,
    unavailable: () => {
      layout = null;
    },
    retire: () => cleanups.forEach((cleanup) => cleanup()),
  };
}
function cells(tree: Element): Element[] {
  return tree.props.albumContent.media.props.children.map(
    (wrapper: Element) => wrapper.props.children,
  );
}
test('album composition forwards canonical identity, actions, every signed caption and a conservative aggregate receipt', () => {
  const f = fixture(),
    value = album(),
    tree = f.render(value),
    parts = cells(tree);
  assert.equal(tree.type, 'CanonicalConversationMessage');
  assert.equal(tree.props.albumContent.delivery, 'Sent');
  assert.deepEqual(
    parts.map((part) => part.props.entry),
    value.entries,
  );
  for (const part of parts)
    for (const key of Object.keys(f.callbacks))
      assert.strictEqual(part.props[key], f.callbacks[key as keyof typeof f.callbacks]);
  assert.deepEqual(Object.keys(parts[0]!.props.albumPart.tile), ['width', 'height']);
  const captions = tree.props.albumContent.captions.props.children as Element[];
  assert.deepEqual(
    captions.map((caption) => caption.props.entry),
    value.entries,
  );
  assert.ok(captions.every((caption) => caption.props.albumPart.captionOnly));
});
test('decoded dimensions must still match the current canonical file version and retired owners cannot update geometry', () => {
  const f = fixture(),
    value = album(),
    old = cells(f.render(value))[0]!;
  const image: DecodedMessageImage = {
    itemId: 'file-0',
    version: 1,
    sha256: 'a'.repeat(64),
    width: 200,
    height: 100,
  };
  old.props.albumPart.onDecoded(image);
  f.render(value);
  assert.equal(
    JSON.stringify(f.geometryInputs.at(-1)),
    JSON.stringify([
      [200, 100],
      [1, 1],
    ]),
  );
  const changed = {
    ...value,
    entries: value.entries.map((entry, index) =>
      index
        ? entry
        : {
            ...entry,
            kind: {
              Message: { text: 'New caption', card: { ...messageCard(entry)!, version: 2 } },
            },
          },
    ),
  };
  f.render(changed);
  const before = f.writes();
  old.props.albumPart.onDecoded({ ...image, width: 999 });
  assert.equal(f.writes(), before);
  assert.equal(
    JSON.stringify(f.geometryInputs.at(-1)),
    JSON.stringify([
      [1, 1],
      [1, 1],
    ]),
  );
  const latest = cells(f.render(changed))[0]!;
  f.retire();
  latest.props.albumPart.onDecoded({ ...image, version: 2 });
  assert.equal(f.writes(), before);
});
test('unavailable mosaic geometry renders every canonical member individually rather than losing hidden projected rows', () => {
  const f = fixture(),
    value = album();
  f.unavailable();
  const result = f.render(value) as unknown as Element[];
  assert.deepEqual(
    result.map((element) => element.props.entry),
    value.entries,
  );
  assert.ok(
    result.every(
      (element) =>
        element.type === 'CanonicalConversationMessage' && element.props.albumContent === undefined,
    ),
  );
});

test('a generic or large member retains every ordinary canonical row and a stale file-version callback cannot disband its successor', () => {
  const f = fixture(),
    value = album(),
    first = cells(f.render(value))[0]!;
  first.props.albumPart.onInlineUnavailable();
  assert.deepEqual(
    (f.render(value) as unknown as Element[]).map((row) => row.props.entry),
    value.entries,
  );
  const changed = {
    ...value,
    entries: value.entries.map((entry, index) =>
      index
        ? entry
        : {
            ...entry,
            kind: {
              Message: { text: 'New caption', card: { ...messageCard(entry)!, version: 2 } },
            },
          },
    ),
  };
  const successor = f.render(changed);
  assert.ok(successor.props.albumContent);
  const before = f.writes();
  first.props.albumPart.onInlineUnavailable();
  assert.equal(f.writes(), before);
  assert.ok(f.render(changed).props.albumContent);
});
