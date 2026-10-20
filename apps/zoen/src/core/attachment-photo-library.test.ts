import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { PhotoLibraryItem } from '../media/attachment-photo-library';
const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../media/attachment-photo-library.ts', import.meta.url),
      'utf8',
    ),
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'actual-library-native-ports',
      setup(b) {
        b.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
type Entry = { name: string; mime: string; size: number; bytes: string };
function fixture() {
  const entries = new Map<string, Entry>(),
    uris = new Map<string, string>(),
    deleted: string[] = [],
    copies: string[][] = [],
    versions: unknown[] = [],
    queries: unknown[][] = [],
    exportsReleased: string[] = [];
  let sequence = 0,
    failCopy = false;
  let getUri = (id: string) => Promise.resolve(uris.get(id)!);
  class File {
    uri: string;
    constructor(base: string | { uri: string }, child?: string) {
      this.uri = (typeof base === 'string' ? base : base.uri) + (child ? child : '');
    }
    get exists() {
      return entries.has(this.uri);
    }
    get size() {
      return entries.get(this.uri)?.size ?? 0;
    }
    get type() {
      return entries.get(this.uri)?.mime ?? '';
    }
    get name() {
      return entries.get(this.uri)?.name ?? this.uri.split('/').at(-1)!;
    }
    copy(destination: File) {
      copies.push([this.uri, destination.uri]);
      entries.set(destination.uri, { ...entries.get(this.uri)! });
      if (failCopy) throw new Error('copy interrupted');
    }
  }
  class Query {
    within(...args: unknown[]) {
      queries.push(['within', ...args]);
      return this;
    }
    orderBy(...args: unknown[]) {
      queries.push(['orderBy', ...args]);
      return this;
    }
    limit(...args: unknown[]) {
      queries.push(['limit', ...args]);
      return this;
    }
    offset(...args: unknown[]) {
      queries.push(['offset', ...args]);
      return this;
    }
    album(...args: unknown[]) {
      queries.push(['album', ...args]);
      return this;
    }
    exeForMetadata() {
      return Promise.resolve([]);
    }
  }
  const core: { releaseMediaLibraryExport?: (uri: string) => Promise<void> } = {
    releaseMediaLibraryExport: async (uri) => {
      exportsReleased.push(uri);
      if (uri.startsWith('file:///owned-tmp/MediaLibraryUriExtractorDirectory/slowMoVideo-'))
        entries.delete(uri);
    },
  };
  const ports: Record<string, unknown> = {
    '../../modules/zoen-core/src/ZoenCoreModule': { __esModule: true, default: core },
    'expo-media-library': {
      Asset: class {
        id: string;
        constructor(id: string) {
          this.id = id;
        }
        getUri(options: unknown) {
          versions.push(options);
          return getUri(this.id);
        }
      },
      Album: class {
        id: string;
        constructor(id: string) {
          this.id = id;
        }
        static getAlbumsMetadata() {
          return Promise.resolve([]);
        }
      },
      AssetField: { MEDIA_TYPE: 'mediaType', CREATION_TIME: 'creationTime' },
      AssetUriVersion: { CURRENT: 'current' },
      MediaType: { IMAGE: 'image', VIDEO: 'video' },
      Query,
      getPermissionsAsync: () => {},
      requestPermissionsAsync: () => {},
      presentPermissionsPicker: () => {},
      addListener: () => ({ remove: () => {} }),
    },
    'expo-file-system': { File, Paths: { cache: { uri: 'file:///cache/' } } },
    'expo-crypto': { randomUUID: () => String(++sequence) },
    './attachment-draft': {
      MAX_ATTACHMENT_TOTAL_BYTES: 64 * 1024 * 1024,
      AttachmentDraftFailure: class extends Error {},
    },
    './cache-files': {
      discardCacheCopy: (uri: string) => {
        deleted.push(uri);
        entries.delete(uri);
      },
    },
  };
  const module = {
    exports: {} as {
      materializeLibrarySelection(
        items: PhotoLibraryItem[],
        current: () => boolean,
      ): Promise<{ uri: string; name: string; mime: string; size: number }[]>;
      photoLibrary: { load(id: string | null, offset: number): Promise<unknown> };
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (id: string) => {
      assert.ok(id in ports, id);
      return ports[id];
    },
  });
  const asset = (id: string, mime = 'image/jpeg', name = `${id}.jpg`, size = 12) => {
    const uri = `file:///Photos/${name}`;
    uris.set(id, uri);
    entries.set(uri, { mime, name, size, bytes: `actual-${id}` });
    return { id, mediaType: mime.startsWith('video/') ? 'video' : 'image' } as PhotoLibraryItem;
  };
  return {
    ...module.exports,
    asset,
    core,
    exportsReleased,
    entries,
    uris,
    deleted,
    copies,
    versions,
    queries,
    fail: () => {
      failCopy = true;
    },
    delay: (read: (id: string) => Promise<string>) => {
      getUri = read;
    },
  };
}
test('actual SDK adapter resolves CURRENT and copies exact local bytes/order/type into fresh owned destinations', async () => {
  const f = fixture(),
    a = f.asset('a'),
    v = f.asset('v', 'video/quicktime', 'current-edited.mov');
  const out = await f.materializeLibrarySelection([v, a], () => true);
  assert.equal(out.length, 2);
  assert.deepEqual(
    Array.from(out, (i) => i.mime),
    ['video/quicktime', 'image/jpeg'],
  );
  assert.ok(out.every((i) => i.uri.startsWith('file:///cache/attachment-')));
  assert.equal(f.entries.get(out[0]!.uri)?.bytes, 'actual-v');
  assert.equal(f.entries.get(out[1]!.uri)?.bytes, 'actual-a');
  assert.equal(out[0]!.name, 'current-edited.mov');
  assert.ok(f.versions.every((o) => (o as { version: string }).version === 'current'));
  assert.deepEqual(f.deleted, []);
  assert.ok(f.entries.has('file:///Photos/current-edited.mov'));
});
test('late iCloud resolution after retirement never copies or deletes the library original', async () => {
  const f = fixture(),
    a = f.asset('a');
  let current = true;
  f.delay(async () => {
    current = false;
    return f.uris.get('a')!;
  });
  assert.deepEqual(Array.from(await f.materializeLibrarySelection([a], () => current)), []);
  assert.deepEqual(f.copies, []);
  assert.deepEqual(f.deleted, []);
  assert.ok(f.entries.has(f.uris.get('a')!));
});
test('partial copy failure removes only fresh destinations and rejects the batch', async () => {
  const f = fixture(),
    a = f.asset('a');
  f.fail();
  await assert.rejects(f.materializeLibrarySelection([a], () => true));
  assert.equal(f.deleted.length, 1);
  assert.ok(f.deleted.every((uri) => uri.startsWith('file:///cache/attachment-')));
  assert.ok(f.entries.has(f.uris.get('a')!));
});
test('foreign URI, unsupported type and aggregate actual-size overflow fail before additional copy effects', async () => {
  for (const kind of ['remote', 'type', 'total']) {
    const f = fixture(),
      a = f.asset('a', 'image/jpeg', 'a.jpg', 40 * 1024 * 1024),
      b = f.asset('b', 'image/jpeg', 'b.jpg', 40 * 1024 * 1024);
    if (kind === 'remote') f.uris.set('a', 'https://untrusted.invalid/photo');
    if (kind === 'type') f.entries.get(f.uris.get('a')!)!.mime = 'application/octet-stream';
    await assert.rejects(
      f.materializeLibrarySelection(kind === 'total' ? [a, b] : [a], () => true),
    );
    assert.equal(f.copies.length, kind === 'total' ? 1 : 0);
    assert.ok(f.deleted.every((uri) => uri.startsWith('file:///cache/attachment-')));
    assert.ok(f.entries.has('file:///Photos/a.jpg'));
  }
});
test('actual new Query uses bounded recent photo/video metadata and album binding without URI downloads', async () => {
  const f = fixture();
  await f.photoLibrary.load('album', 60);
  assert.equal(f.versions.length, 0);
  assert.ok(f.queries.some((q) => q[0] === 'limit' && q[1] === 60));
  assert.ok(f.queries.some((q) => q[0] === 'offset' && q[1] === 60));
  assert.ok(f.queries.some((q) => q[0] === 'album' && (q[1] as { id: string }).id === 'album'));
});

test('CURRENT composition export is released after exact-byte copy while Photos originals remain untouched', async () => {
  const f = fixture(),
    video = f.asset('composition', 'video/quicktime', 'current.mov');
  const original = f.uris.get('composition')!;
  const sdk =
    'file:///owned-tmp/MediaLibraryUriExtractorDirectory/slowMoVideo-11111111-2222-3333-4444-555555555555.mov';
  f.entries.set(sdk, { ...f.entries.get(original)! });
  f.uris.set('composition', sdk);
  const result = await f.materializeLibrarySelection([video], () => true);
  assert.equal(result.length, 1);
  assert.deepEqual(f.exportsReleased, [sdk]);
  assert.equal(f.entries.has(sdk), false);
  assert.equal(f.entries.get(result[0]!.uri)?.bytes, 'actual-composition');
  assert.ok(f.entries.has(original));
});
test('late CURRENT export, failed copy and invalid actual metadata each release this operation returned export', async () => {
  for (const mode of ['late', 'copy', 'metadata']) {
    const f = fixture(),
      video = f.asset('composition', 'video/quicktime', 'current.mov');
    let current = true;
    const original = f.uris.get('composition')!,
      sdk =
        'file:///owned-tmp/MediaLibraryUriExtractorDirectory/slowMoVideo-11111111-2222-3333-4444-555555555555.mov';
    f.entries.set(sdk, { ...f.entries.get(original)!, size: mode === 'metadata' ? 0 : 12 });
    f.uris.set('composition', sdk);
    if (mode === 'late')
      f.delay(async () => {
        current = false;
        return sdk;
      });
    if (mode === 'copy') f.fail();
    if (mode === 'late')
      assert.equal((await f.materializeLibrarySelection([video], () => current)).length, 0);
    else await assert.rejects(f.materializeLibrarySelection([video], () => current));
    assert.deepEqual(f.exportsReleased, [sdk]);
    assert.equal(f.entries.has(sdk), false);
    assert.ok(f.entries.has(original));
  }
});
test('missing native release capability prevents starting SDK exports and native release failure prevents draft admission', async () => {
  const f = fixture(),
    video = f.asset('v', 'video/quicktime', 'v.mov');
  f.core.releaseMediaLibraryExport = undefined;
  await assert.rejects(f.materializeLibrarySelection([video], () => true));
  assert.equal(f.versions.length, 0);
  assert.equal(f.copies.length, 0);
  f.core.releaseMediaLibraryExport = async () => {
    throw new Error('owned export cleanup denied');
  };
  await assert.rejects(f.materializeLibrarySelection([video], () => true));
  assert.equal(f.copies.length, 1);
  assert.ok(f.deleted.some((uri) => uri.startsWith('file:///cache/attachment-')));
  assert.ok(f.entries.has(f.uris.get('v')!));
});
