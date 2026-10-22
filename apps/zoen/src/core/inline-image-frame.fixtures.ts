import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import type { previewConversationMedia as ConversationPreview } from '../media/conversation-media-preview';
import { formatFileSize } from '../media/file-size.ts';
import type { DecodedMessageImage } from '../media/inline-image-frame.ts';
import type { LocalMediaFile } from '../media/types.ts';
import { resolveSkyTheme } from '../theme/sky.ts';
import type { FileDto } from './generated/FileDto.ts';
import { albumDelivery, type MediaAlbum } from './media-albums.ts';
import { messageCard } from './message-presentation.ts';
import { messageText } from './presentation.ts';
import type { ClientStore } from './store';
import type { TimelineEntry } from './types';

export const sourcePath = fileURLToPath(new URL('../media/MediaAttachment.tsx', import.meta.url));
export const require = createRequire(import.meta.url);
export const { transformSync } = require('@babel/core');
export const code = transformSync(
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
export const compiled = await build({
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
export const albumCode = (
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
export const conversationCode = (
  await build({
    stdin: {
      contents: readFileSync(
        new URL('../media/conversation-media-preview.ts', import.meta.url),
        'utf8',
      ),
      loader: 'ts',
    },
    bundle: false,
    platform: 'node',
    format: 'cjs',
    write: false,
  })
).outputFiles[0]!.text;
export const conversationModule = {
  exports: {} as { previewConversationMedia: typeof ConversationPreview },
};
runInNewContext(conversationCode, {
  module: conversationModule,
  exports: conversationModule.exports,
  AbortController,
  require: (name: string) => {
    if (name === '../i18n/index.ts') return { t: (key: string) => key };
    assert.equal(name, './playback-owner');
    return { mediaPlayback: { pauseAll() {} } };
  },
});
export type Element = { type: unknown; props: Record<string, unknown> };
export function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
export function style(node: Element) {
  const value = node.props.style;
  return (typeof value === 'function' ? value({ pressed: false }) : value) as Record<
    string,
    unknown
  >;
}
export const tick = async () => {
  for (let i = 0; i < 12; i++) await Promise.resolve();
};
export function fixture(
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
    galleryEntries?: readonly TimelineEntry[];
    actualGallery?: boolean;
    tile?: { width: number; height: number };
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
    previewConversationMedia: options.galleryCapability
      ? async (_request: unknown, signal: AbortSignal) => {
          if (!options.actualGallery) return;
          await new Promise<void>((resolve, reject) => {
            nativeCompletion = { resolve, reject };
            signal.addEventListener(
              'abort',
              () => reject(new Error('Synthetic native cancellation')),
              { once: true },
            );
          });
        }
      : undefined,
  };
  let nativeCompletion: { resolve(): void; reject(reason: Error): void } | undefined;
  let retiring = false;
  const stateListeners = new Set<() => void>();
  const canonicalListeners = new Set<(spaces?: readonly string[]) => void>();
  const cleanups = new Set<() => Promise<void>>();
  const store = {
    sessionEpoch: 1,
    get isSessionRetiring() {
      return retiring;
    },
    getSnapshot: () => ({ phase: 'ready', erasePending: false }),
    subscribe: (listener: () => void) => {
      stateListeners.add(listener);
      return () => stateListeners.delete(listener);
    },
    subscribeCanonicalChanges: (listener: (spaces?: readonly string[]) => void) => {
      canonicalListeners.add(listener);
      return () => canonicalListeners.delete(listener);
    },
    registerSessionCleanup: (cleanup: () => Promise<void>) => {
      cleanups.add(cleanup);
      return () => cleanups.delete(cleanup);
    },
    driver: {
      media: driver,
      request: async (request: { message_id: string }) =>
        structuredClone(
          (options.galleryEntries ?? [options.messageSource?.entry]).find(
            (entry) => entry?.id === request.message_id,
          ),
        ),
    },
  } as unknown as ClientStore;
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
      useClient: () => ({ store, snapshot, mode: 'account' }),
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
        if (options.actualGallery)
          await conversationModule.exports.previewConversationMedia(
            ...(args as Parameters<typeof ConversationPreview>),
          );
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
      galleryEntries: options.galleryEntries,
      tile: options.tile,
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
    nativeIsOpen: () => nativeCompletion != null,
    rejectNative: (reason: Error) => {
      assert.ok(nativeCompletion);
      nativeCompletion.reject(reason);
    },
    abortGallery: () => {
      retiring = true;
      for (const listener of stateListeners) listener();
    },
    galleryListenerCount: () => stateListeners.size + canonicalListeners.size + cleanups.size,
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
export const near = (actual: unknown, expected: number) =>
  assert.ok(Math.abs(Number(actual) - expected) < 1e-6, `${actual} should be ${expected}`);
