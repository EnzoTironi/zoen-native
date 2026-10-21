import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import type { PlaybackCoordinator } from '../media/playback-owner';
import { build } from 'esbuild';
import type { LocalMediaFile, MediaDriver } from '../media/types';

type Options = { mimeType: string; title?: string };
type Event = { kind: string; uri: string };
type SDKFixture = {
  events: Event[];
  checks: { uri: string; options: Options }[];
  previews: { uri: string; options: Options }[];
  canPreview(uri: string, options: Options): Promise<boolean>;
  preview(uri: string, options: Options): Promise<void>;
};
const globals = globalThis as typeof globalThis & { __zoenMediaPreviewFixture?: SDKFixture };
const output = await build({
  stdin: {
    contents:
      "export { previewMediaFile } from '../media/preview-file.ts'; export { mediaPlayback } from '../media/playback-owner.ts';",
    resolveDir: fileURLToPath(new URL('.', import.meta.url)),
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
  plugins: [
    {
      name: 'media-preview-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /^(expo-file-system|\.\.\/i18n\/index\.ts)$/ }, (args) => ({
          path: args.path,
          namespace: 'preview-fixture',
        }));
        builder.onLoad({ filter: /.*/, namespace: 'preview-fixture' }, ({ path }) => ({
          loader: 'js',
          contents:
            path === 'expo-file-system'
              ? `
                export class File {
                  constructor(uri) {
                    this.uri = uri;
                    globalThis.__zoenMediaPreviewFixture.events.push({ kind: 'sdk-create', uri });
                  }
                  async canPreview(options) {
                    const f = globalThis.__zoenMediaPreviewFixture;
                    f.events.push({ kind: 'sdk-check', uri: this.uri });
                    f.checks.push({ uri: this.uri, options });
                    return f.canPreview(this.uri, options);
                  }
                  async preview(options) {
                    const f = globalThis.__zoenMediaPreviewFixture;
                    f.events.push({ kind: 'sdk-preview', uri: this.uri });
                    f.previews.push({ uri: this.uri, options });
                    return f.preview(this.uri, options);
                  }
                }
              `
              : 'export const t = (key) => key;',
        }));
      },
    },
  ],
});
const { previewMediaFile, mediaPlayback } = (await import(
  `data:text/javascript;base64,${Buffer.from(output.outputFiles[0]!.text).toString('base64')}`
)) as {
  previewMediaFile(media: MediaDriver, file: LocalMediaFile): Promise<void>;
  mediaPlayback: PlaybackCoordinator;
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

const file: LocalMediaFile = {
  uri: 'file:///synthetic/preview-copy.pdf',
  itemId: 'synthetic_document',
  version: 2,
  info: {
    name: 'Synthetic report.pdf',
    path: 'Synthetic report.pdf',
    mime: 'application/pdf',
    bytes: '1000',
    sha256: 'a'.repeat(64),
    ready: true,
    chunks: 1,
    chunks_here: 1,
    has_thumbnail: false,
  },
};

function fixture(native?: (value: LocalMediaFile) => Promise<void>) {
  const sdk: SDKFixture = {
    events: [],
    checks: [],
    previews: [],
    canPreview: async () => true,
    preview: async () => {},
  };
  globals.__zoenMediaPreviewFixture = sdk;
  const released: string[] = [];
  let cleanupError: unknown;
  const media: MediaDriver = {
    sendFile: async () => assert.fail('Preview must not send a file'),
    sendVoice: async () => assert.fail('Preview must not send audio'),
    fileInfo: async () => assert.fail('The exact signed file is already supplied'),
    materializeFile: async () => assert.fail('Preview must not create another lease'),
    releaseLocalFile: async (uri) => {
      released.push(uri);
      sdk.events.push({ kind: 'release', uri });
      if (cleanupError) throw cleanupError;
    },
  };
  if (native) media.previewFile = native;
  return {
    sdk,
    media,
    released,
    failCleanup: (reason: unknown) => {
      cleanupError = reason;
    },
  };
}

test('native preview keeps its independent lease until dismissal after the row drops its own copy', async () => {
  const closed = deferred<void>();
  const f = fixture(async (value) => {
    assert.equal(value, file);
    f.sdk.events.push({ kind: 'native-open', uri: value.uri });
    await closed.promise;
    f.sdk.events.push({ kind: 'native-close', uri: value.uri });
  });
  const preview = previewMediaFile(f.media, file);
  await f.media.releaseLocalFile('file:///synthetic/row-copy.pdf');
  assert.deepEqual(f.released, ['file:///synthetic/row-copy.pdf']);
  assert.deepEqual(f.sdk.checks, []);
  assert.deepEqual(f.sdk.previews, []);
  closed.resolve();
  await preview;
  assert.deepEqual(f.released, ['file:///synthetic/row-copy.pdf', file.uri]);
  assert.deepEqual(
    f.sdk.events.filter((event) => event.uri === file.uri),
    [
      { kind: 'native-open', uri: file.uri },
      { kind: 'native-close', uri: file.uri },
      { kind: 'release', uri: file.uri },
    ],
  );
});

test('native rejection releases once, never starts the SDK viewer, and does not block a fresh retry', async () => {
  const rejected = deferred<void>();
  const nativeError = new Error('Synthetic native presenter failure');
  let attempts = 0;
  const f = fixture(async () => {
    if (++attempts === 1) await rejected.promise;
  });
  const first = previewMediaFile(f.media, file);
  assert.deepEqual(f.released, []);
  rejected.reject(nativeError);
  await assert.rejects(first, (reason) => reason === nativeError);
  assert.deepEqual(f.released, [file.uri]);
  const retry = { ...file, uri: 'file:///synthetic/fresh-preview-copy.pdf' };
  await previewMediaFile(f.media, retry);
  assert.equal(attempts, 2);
  assert.deepEqual(f.released, [file.uri, retry.uri]);
  assert.equal(
    f.sdk.events.some((event) => event.kind.startsWith('sdk-')),
    false,
  );
});

test('a synchronous native presenter failure propagates without falling back or leaking the lease', async () => {
  const nativeError = new Error('Synthetic presenter unavailable');
  const f = fixture(() => {
    throw nativeError;
  });
  await assert.rejects(previewMediaFile(f.media, file), (reason) => reason === nativeError);
  assert.deepEqual(f.released, [file.uri]);
  assert.deepEqual(f.sdk.events, [{ kind: 'release', uri: file.uri }]);
});

test('SDK fallback checks support and waits for native dismissal with the signed title and MIME', async () => {
  const supported = deferred<boolean>();
  const entered = deferred<void>();
  const closed = deferred<void>();
  const f = fixture();
  f.sdk.canPreview = () => supported.promise;
  f.sdk.preview = async () => {
    entered.resolve();
    await closed.promise;
  };
  const preview = previewMediaFile(f.media, file);
  assert.deepEqual(f.sdk.checks, [{ uri: file.uri, options: { mimeType: file.info.mime } }]);
  assert.deepEqual(f.sdk.previews, []);
  assert.deepEqual(f.released, []);
  supported.resolve(true);
  await entered.promise;
  assert.deepEqual(f.sdk.previews, [
    { uri: file.uri, options: { title: file.info.name, mimeType: file.info.mime } },
  ]);
  assert.deepEqual(f.released, []);
  closed.resolve();
  await preview;
  assert.deepEqual(f.released, [file.uri]);
  assert.deepEqual(
    f.sdk.events.map((event) => event.kind),
    ['sdk-create', 'sdk-check', 'sdk-preview', 'release'],
  );
});

test('an unsupported fallback reports the existing error without presenting and releases once', async () => {
  const f = fixture();
  f.sdk.canPreview = async () => false;
  await assert.rejects(previewMediaFile(f.media, file), {
    message: 'media.thisFormatCannotBePreviewedOnThisDevice',
  });
  assert.deepEqual(f.sdk.previews, []);
  assert.deepEqual(f.released, [file.uri]);
});

test('fallback support-query and viewer failures both release the supplied lease once', async () => {
  for (const stage of ['canPreview', 'preview'] as const) {
    const f = fixture();
    const error = new Error(`Synthetic ${stage} failure`);
    f.sdk[stage] = async () => {
      throw error;
    };
    await assert.rejects(previewMediaFile(f.media, file), (reason) => reason === error);
    assert.deepEqual(f.released, [file.uri]);
    assert.equal(f.sdk.previews.length, stage === 'preview' ? 1 : 0);
  }
});

test('cleanup failure surfaces only after the native preview has finished and is not retried implicitly', async () => {
  const closed = deferred<void>();
  const f = fixture(() => closed.promise);
  const cleanupError = new Error('Synthetic local-copy cleanup failure');
  f.failCleanup(cleanupError);
  const preview = previewMediaFile(f.media, file);
  assert.deepEqual(f.released, []);
  closed.resolve();
  await assert.rejects(preview, (reason) => reason === cleanupError);
  assert.deepEqual(f.released, [file.uri]);
});

test('opening the native visual gallery synchronously interrupts actual playback owners before admission and retains its lease until dismissal', async () => {
  const closed = deferred<void>();
  let playing = true;
  const f = fixture(async () => {
    f.sdk.events.push({ kind: 'native-open', uri: file.uri });
    assert.equal(playing, false);
    await closed.promise;
  });
  const owner = mediaPlayback.register(
    () => {
      playing = false;
      f.sdk.events.push({ kind: 'pause-player', uri: file.uri });
    },
    () => playing,
  );
  const intent = owner.begin();
  try {
    const result = previewMediaFile(f.media, file);
    assert.deepEqual(
      f.sdk.events.map((event) => event.kind),
      ['pause-player', 'native-open'],
    );
    assert.equal(owner.isCurrent(intent), false);
    assert.deepEqual(f.released, []);
    closed.resolve();
    await result;
    assert.deepEqual(f.released, [file.uri]);
  } finally {
    closed.resolve();
    owner.dispose();
  }
});
