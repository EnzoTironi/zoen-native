import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { FileDto } from './generated/FileDto.ts';
import type { LocalMediaFile } from '../media/types.ts';
import type { DecodedMessageImage } from '../media/inline-image-frame.ts';
import { contrastRatio, resolveSkyTheme, skyBackdrops } from '../theme/sky.ts';
import { DEFAULT_THEME_CUSTOMIZATION } from '../theme/telegram.ts';
import { formatFileSize } from '../media/file-size.ts';
import { albumDelivery, type MediaAlbum } from './media-albums.ts';
import { messageCard } from './message-presentation.ts';
import { messageText } from './presentation.ts';
import type { TimelineEntry } from './types';

const sourcePath = fileURLToPath(new URL('../media/MediaAttachment.tsx', import.meta.url));
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const code = transformSync(
  `${readFileSync(process.env.ZOEN_MEDIA_ATTACHMENT_SOURCE ?? sourcePath, 'utf8')}\nexport { FileAttachmentContent };`,
  {
    filename: sourcePath,
    configFile: false,
    babelrc: false,
    presets: [require.resolve('babel-preset-expo')],
    caller: {
      name: 'metro',
      platform: 'ios',
      engine: 'hermes',
      isDev: false,
      supportsReactCompiler: true,
    },
  },
).code as string;
assert.match(code, /react\/compiler-runtime/, 'Actual installed iOS Babel/React compiler');
const compiled = await build({
  stdin: {
    contents: code,
    resolveDir: dirname(sourcePath),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'inline-image-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          path === './inline-image-frame' || path.startsWith('@babel/runtime/helpers/')
            ? undefined
            : { path, external: true },
        );
      },
    },
  ],
});
const albumCode = (
  await build({
    stdin: {
      contents: readFileSync(
        new URL('../components/conversation-album-message.tsx', import.meta.url),
        'utf8',
      ),
      loader: 'tsx',
    },
    bundle: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
  })
).outputFiles[0]!.text;
type Element = { type: unknown; props: Record<string, unknown> };
function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
function style(node: Element) {
  const value = node.props.style;
  return (typeof value === 'function' ? value({ pressed: false }) : value) as Record<
    string,
    unknown
  >;
}
const tick = async () => {
  for (let i = 0; i < 12; i++) await Promise.resolve();
};
function fixture(
  mine = true,
  options: {
    theme?: ReturnType<typeof resolveSkyTheme>;
    info?: Partial<FileDto>;
    voice?: Record<string, unknown>;
    onInlineUnavailable?: () => void;
    fileInfoError?: boolean;
    materializeError?: boolean;
    messageSource?: { spaceId: string; entry: TimelineEntry };
    galleryCapability?: boolean;
  } = {},
) {
  const info: FileDto = {
    name: 'photo.jpg',
    path: '',
    mime: 'image/jpeg',
    bytes: '12',
    sha256: 'a'.repeat(64),
    ready: true,
    chunks: 1,
    chunks_here: 1,
    has_thumbnail: false,
    ...options.info,
  };
  const file: LocalMediaFile = {
    uri: 'file:///synthetic/verified-photo.jpg',
    itemId: 'item',
    version: 3,
    info,
  };
  const decoded: DecodedMessageImage[] = [];
  const legacyPreviews: LocalMediaFile[] = [];
  const galleryRequests: unknown[][] = [];
  const materialized: { itemId: string; version?: number }[] = [];
  const slots: { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void }[] = [];
  let cursor = 0;
  let compilerCursor = 0;
  const caches: unknown[][] = [];
  let effects: (() => void)[] = [];
  const slot = () => (slots[cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    !!a && !!b && a.length === b.length && a.every((value, index) => Object.is(value, b[index]));
  const effect = (work: () => void | (() => void), deps: readonly unknown[]) => {
    const current = slot();
    if (same(current.deps, deps)) return;
    current.deps = deps;
    effects.push(() => {
      current.cleanup?.();
      current.cleanup = work() ?? undefined;
    });
  };
  const driver = {
    fileInfo: async () => {
      if (options.fileInfoError) throw new Error('Synthetic metadata unavailable');
      return info;
    },
    materializeFile: async (itemId: string, version?: number) => {
      materialized.push({ itemId, version });
      if (options.materializeError) throw new Error('Synthetic copy unavailable');
      return file;
    },
    releaseLocalFile: async () => {},
    previewConversationMedia: options.galleryCapability ? async () => {} : undefined,
  };
  const snapshot = { account: null };
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const ports: Record<string, unknown> = {
    react: {
      useState: (initial: unknown) => {
        const current = slot();
        if (!Object.hasOwn(current, 'value')) current.value = initial;
        return [
          current.value,
          (value: unknown) => {
            current.value = typeof value === 'function' ? value(current.value) : value;
          },
        ];
      },
      useRef: (value: unknown) => {
        const current = slot();
        return (current.value ??= { current: value });
      },
      useEffect: effect,
      useLayoutEffect: effect,
      useCallback: (work: unknown) => work,
    },
    'react/compiler-runtime': {
      c: (size: number) =>
        (caches[compilerCursor++] ??= Array(size).fill(Symbol.for('react.memo_cache_sentinel'))),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': {
      View: 'View',
      Text: 'Text',
      ActivityIndicator: 'ActivityIndicator',
      useWindowDimensions: () => ({ width: 390, scale: 3 }),
    },
    'expo-image': { Image: 'Image' },
    'expo-sharing': {},
    '../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en-US' },
    '../components/message-interaction': { MessagePressable: 'MessagePressable' },
    '../core/provider': {
      useClient: () => ({ store: { driver: { media: driver } }, snapshot, mode: 'account' }),
    },
    '../theme': {
      useTheme: () => options.theme ?? resolveSkyTheme('night', new Date('2026-10-18T12:00:00')),
    },
    '../components/primitives': { Icon: 'Icon' },
    './AudioClip': {
      AudioClip: 'AudioClip',
      AudioClipControls: 'AudioClipControls',
      clipTime: () => '',
    },
    './message-audio-provider': { useReceivedMessageAudio: () => null },
    './audio-progress': { AudioWaveform: 'AudioWaveform' },
    './voice-transcript': { VoiceTranscript: 'VoiceTranscript' },
    './VideoClip': { VideoClip: 'VideoClip' },
    './preview-file': {
      previewMediaFile: async (_driver: unknown, file: LocalMediaFile) => {
        legacyPreviews.push(file);
      },
    },
    './conversation-media-preview': {
      previewConversationMedia: async (...args: unknown[]) => {
        if (!options.galleryCapability)
          throw new Error('Unexpected conversation viewer admission in inline rendering test');
        galleryRequests.push(args);
      },
    },
    './file-size': { formatFileSize },
    '../core/media-albums': { albumDelivery },
    '../core/message-presentation': { messageCard },
    '../core/presentation': { messageText },
    './conversation-message': { ConversationMessage: 'CanonicalConversationMessage' },
    '../media/mosaic-layout': {
      mediaMosaicLayout: () => ({
        width: 201,
        height: 100,
        items: [
          { x: 0, y: 0, width: 100, height: 100, position: 1 },
          { x: 101, y: 0, width: 100, height: 100, position: 2 },
        ],
      }),
    },
  };
  const module = {
    exports: {} as { FileAttachmentContent(props: Record<string, unknown>): Element },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  const albumModule = {
    exports: {} as {
      ConversationAlbumMessage(props: Record<string, unknown>): Element | Element[];
    },
  };
  runInNewContext(albumCode, {
    module: albumModule,
    exports: albumModule.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const render = () => {
    cursor = 0;
    compilerCursor = 0;
    effects = [];
    const tree = module.exports.FileAttachmentContent({
      itemId: 'item',
      version: 3,
      presentation: 'message',
      mine,
      voice: options.voice,
      onImageDecoded: (image: DecodedMessageImage) => decoded.push(image),
      onInlineUnavailable: options.onInlineUnavailable,
      messageSource: options.messageSource,
    });
    for (const work of effects) work();
    return tree;
  };
  const parts = () => {
    const tree = render();
    const image = elements(tree).find((node) => node.type === 'Image')!;
    const frame = elements(tree).find((node) => node.type === 'MessagePressable')!;
    assert.ok(image && frame, 'Actual message image was materialized');
    const mask = elements(frame.props.children).find((node) => node.type === 'View')!;
    return { tree, image, frame, mask };
  };
  return {
    render,
    renderAlbum: (album: MediaAlbum) => {
      cursor = 0;
      compilerCursor = 0;
      effects = [];
      const tree = albumModule.exports.ConversationAlbumMessage({
        album,
        spaceId: album.spaceId,
        entry: album.entries[0],
      });
      for (const work of effects) work();
      return tree;
    },
    parts,
    file,
    decoded,
    legacyPreviews,
    galleryRequests,
    materialized,
    ready: async () => {
      render();
      await tick();
      render();
      await tick();
      render();
    },
    layout: (width: number) => {
      const tree = render();
      // The original component has no width measurement; let geometry assertions
      // expose its missing frame instead of failing on the callback itself.
      (tree.props.onLayout as ((event: unknown) => void) | undefined)?.({
        nativeEvent: { layout: { width } },
      });
    },
    load: (width: number, height: number, url = file.uri) => {
      (parts().image.props.onLoad as (event: unknown) => void)({ source: { url, width, height } });
    },
    unmount: () => {
      for (const current of slots) current.cleanup?.();
    },
  };
}
const near = (actual: unknown, expected: number) =>
  assert.ok(Math.abs(Number(actual) - expected) < 1e-6, `${actual} should be ${expected}`);

test('actual image/video Open preserves the exact single-file preview for forwarded visual messages with gallery capability present', async () => {
  for (const mime of ['image/jpeg', 'video/mp4']) {
    const entry = { id: 'forwarded-message', forwarded: true, voice: null } as TimelineEntry;
    const f = fixture(false, {
      info: { mime },
      galleryCapability: true,
      messageSource: { spaceId: 'qa', entry },
    });
    await f.ready();
    const button = mime.startsWith('image/')
      ? f.parts().frame
      : elements(f.render()).find((node) => node.props.testID === 'message-video-gallery-item')!;
    assert.ok(button, 'Actual visual media Open control');
    (button.props.onPress as () => void)();
    await tick();
    assert.equal(
      f.galleryRequests.length,
      0,
      'Forwarded media must not enter signed-album request helper',
    );
    assert.equal(f.legacyPreviews.length, 1);
    assert.strictEqual(f.legacyPreviews[0], f.file);
    assert.deepEqual(f.materialized.at(-1), { itemId: 'item', version: 3 });
    f.unmount();
  }
});
test('actual ordinary image Open still dispatches the chosen canonical message to the gallery helper', async () => {
  const entry = { id: 'ordinary-message', forwarded: false, voice: null } as TimelineEntry;
  const f = fixture(false, { galleryCapability: true, messageSource: { spaceId: 'qa', entry } });
  await f.ready();
  (f.parts().frame.props.onPress as () => void)();
  await tick();
  assert.equal(f.legacyPreviews.length, 0);
  assert.equal(f.galleryRequests.length, 1);
  assert.equal(f.galleryRequests[0]![1], 'qa');
  assert.equal(f.galleryRequests[0]![2], entry.id);
  assert.strictEqual((f.galleryRequests[0]![3] as TimelineEntry[])[0], entry);
  f.unmount();
});

test('actual inline image resolves one bounded frame and both image and rounded mask fill it without cropping', async () => {
  for (const mine of [true, false]) {
    const f = fixture(mine);
    await f.ready();
    f.layout(294);
    for (const [width, height, expectedWidth, expectedHeight] of [
      [700, 1000, 252, 360],
      [600, 1000, 216, 360],
      [1000, 1000, 294, 294],
      [2000, 1000, 294, 147],
      [100, 1000, 36, 360],
    ]) {
      f.load(width!, height!);
      const { frame, image, mask } = f.parts();
      near(style(frame).width, expectedWidth!);
      near(style(frame).height, expectedHeight!);
      near(Number(style(frame).width) / Number(style(frame).height), width! / height!);
      assert.equal(style(frame).alignSelf, mine ? 'flex-end' : 'flex-start');
      assert.equal(style(frame).overflow, 'hidden');
      assert.equal(style(frame).borderRadius, style(mask).borderRadius);
      assert.equal(style(image).position, 'absolute');
      assert.equal(style(image).inset, 0);
      assert.equal(style(mask).inset, 0);
      assert.equal(style(image).aspectRatio, undefined);
      assert.equal(style(image).maxHeight, undefined);
      assert.equal(image.props.contentFit, 'contain');
    }
    assert.equal(f.decoded.at(-1)!.sha256, f.file.info.sha256);
    assert.equal(f.decoded.at(-1)!.itemId, 'item');
    assert.equal(f.decoded.at(-1)!.version, 3);
    f.unmount();
  }
});

test('actual inline image recomputes both dimensions after available width changes and ignores invalid or stale load events', async () => {
  const f = fixture();
  await f.ready();
  f.layout(294);
  f.load(2000, 1000);
  near(style(f.parts().frame).height, 147);
  f.layout(144);
  near(style(f.parts().frame).width, 144);
  near(style(f.parts().frame).height, 72);
  f.layout(294);
  near(style(f.parts().frame).width, 294);
  f.load(700, 1000);
  const accepted = f.decoded.length;
  f.load(1000, 1, 'file:///synthetic/old-source.jpg');
  f.load(0, 1000);
  f.load(Number.NaN, 1000);
  assert.equal(f.decoded.length, accepted);
  near(style(f.parts().frame).width, 252);
  f.layout(Number.NaN);
  near(style(f.parts().frame).width, 252);
  const late = f.parts().image.props.onLoad as (event: unknown) => void;
  f.unmount();
  late({ source: { url: f.file.uri, width: 1000, height: 1 } });
  assert.equal(f.decoded.length, accepted);
});

test('generic files and oversized image fallbacks use resolved sky ink independently of outgoing bubble color', async () => {
  for (const phase of ['night', 'day'] as const) {
    for (const outgoing of ['#FFFFFF', '#000000']) {
      const theme = resolveSkyTheme(phase, new Date('2026-10-18T12:00:00'), {
        ...DEFAULT_THEME_CUSTOMIZATION,
        outgoing,
      });
      for (const mine of [true, false]) {
        for (const mime of ['image/png', 'application/pdf']) {
          const f = fixture(mine, { theme, info: { name: 'large-file', mime, bytes: '52533657' } });
          await f.ready();
          const tree = f.render();
          assert.equal(
            elements(tree).some((node) => node.type === 'Image'),
            false,
          );
          const icon = elements(tree).find((node) => node.type === 'Icon')!;
          const [name, size] = elements(tree).filter((node) => node.type === 'Text');
          assert.equal(size!.props.children, formatFileSize(f.file.info.bytes, 'en-US'));
          assert.equal(style(tree).backgroundColor, 'transparent');
          assert.equal(icon.props.color, theme.colors.text);
          assert.equal(style(name!).color, theme.colors.text);
          assert.equal(style(size!).color, theme.colors.secondary);
          assert.equal(
            style(size!).opacity,
            undefined,
            'The resolved secondary ink already accounts for the sky',
          );
          for (const background of skyBackdrops(theme.sky)) {
            assert.ok(contrastRatio(style(name!).color as string, background) >= 4.5);
            assert.ok(contrastRatio(style(size!).color as string, background) >= 4.5);
          }
          f.unmount();
        }
      }
    }
  }
});

test('outgoing voice fallback retains its resolved bubble ink', async () => {
  const theme = resolveSkyTheme('night', new Date('2026-10-18T12:00:00'), {
    ...DEFAULT_THEME_CUSTOMIZATION,
    outgoing: '#FFFFFF',
  });
  const f = fixture(true, {
    theme,
    info: { mime: 'audio/aac' },
    voice: { ms: '1000', levels: [0.2] },
  });
  await f.ready();
  const icon = elements(f.render()).find((node) => node.type === 'Icon')!;
  assert.equal(icon.props.color, theme.colors.onMine);
  f.unmount();
});

test('actual canonical file metadata releases a visual album to individual rows for generic or oversized members only', async () => {
  for (const [mime, bytes, expected] of [
    ['application/pdf', '1000', 1],
    ['audio/mp4', '1000', 1],
    ['image/png', String(16 * 1048576 + 1), 1],
    ['image/png', '1000', 0],
    ['video/mp4', '1000', 0],
  ] as const) {
    let unavailable = 0;
    const f = fixture(true, {
      info: { mime, bytes },
      onInlineUnavailable: () => {
        unavailable++;
      },
    });
    f.render();
    assert.equal(unavailable, 0, 'Unknown metadata must not invent a file classification');
    await f.ready();
    assert.equal(unavailable, expected, mime);
    f.render();
    assert.equal(
      unavailable,
      expected,
      'Unchanged metadata must not publish another classification',
    );
    f.unmount();
  }
});

test('actual metadata, materialize and image decode failures invoke the production album callback and retain EVERY canonical row', async () => {
  for (const failure of ['fileInfoError', 'materializeError', 'decode'] as const) {
    const parent = fixture();
    const album: MediaAlbum = {
      id: 'qa-signed-group',
      spaceId: 'qa',
      presentation: 'media',
      entries: [0, 1].map(
        (index) =>
          ({
            id: `member-${index}`,
            author: { is_me: true },
            delivery: 'Sent',
            kind: {
              Message: {
                text: `Caption ${index}`,
                card: { item_id: index ? 'other' : 'item', kind_id: 'file', version: 3 },
              },
            },
          }) as TimelineEntry,
      ),
    };
    const first = parent.renderAlbum(album) as Element;
    const media = first.props.albumContent as { media: Element };
    const leaf = (media.media.props.children as Element[])[0]!.props.children as Element;
    const part = leaf.props.albumPart as { onInlineUnavailable(): void };
    const file = fixture(true, {
      onInlineUnavailable: part.onInlineUnavailable,
      ...(failure === 'decode' ? {} : { [failure]: true }),
    });
    await file.ready();
    if (failure === 'decode') {
      const failedImage = file.parts().image;
      (failedImage.props.onError as () => void)();
      file.render();
    }
    const rows = parent.renderAlbum(album) as Element[];
    assert.ok(Array.isArray(rows), `${failure} must release the clipped mosaic`);
    assert.deepEqual(
      rows.map((row) => row.props.entry),
      album.entries,
    );
    assert.ok(rows.every((row) => row.type === 'CanonicalConversationMessage'));
    // A late error from the old version cannot disband a newly rendered album.
    const successor = {
      ...album,
      entries: album.entries.map((entry, index) =>
        index
          ? entry
          : {
              ...entry,
              kind: {
                Message: { text: 'Successor', card: { ...messageCard(entry)!, version: 4 } },
              },
            },
      ),
    };
    assert.ok(!Array.isArray(parent.renderAlbum(successor)));
    part.onInlineUnavailable();
    assert.ok(!Array.isArray(parent.renderAlbum(successor)));
    file.unmount();
    parent.unmount();
  }
});
