import assert from 'node:assert/strict';
import { test, type TestContext } from 'node:test';
import { createBrowserMediaDriver } from '../media/driver.web.ts';
import { MAX_SELECTED_BYTES } from '../media/types.ts';
import type { BrowserMediaHost } from './browser-media.ts';

const file = {
  name: 'signed.txt',
  path: '',
  mime: 'text/plain',
  bytes: 3,
  sha256: 'ab'.repeat(32),
  chunks: [{ sha256: 'ab'.repeat(32), bytes: 3, blob: 'cd'.repeat(32), key: 'ef'.repeat(32) }],
  thumb: null,
  voice: null,
};
function fixture() {
  const controller = new AbortController();
  const calls: Parameters<BrowserMediaHost['sendFile']>[] = [];
  const host: BrowserMediaHost = {
    closed: controller.signal,
    async sendFile(...args) {
      calls.push(args);
      return { itemId: 'it_signed', messageId: 'msg_signed' };
    },
    async file(itemId, version) {
      return {
        itemId,
        version: version ?? 7,
        doc: file,
        info: { ...file, bytes: '3', ready: true, chunks: 1, chunks_here: 1, has_thumbnail: false },
      };
    },
    async openFile() {
      return new Blob(['abc'], { type: 'text/plain' });
    },
  };
  return { host, controller, calls };
}
function browser(t: TestContext) {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  Object.defineProperty(globalThis, 'window', {
    configurable: true,
    value: { location: { origin: 'https://zoen.test' } },
  });
  t.after(() => {
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  });
}

test('media ingress refuses remote URLs and oversized local payloads before Rust or network upload', async (t) => {
  browser(t);
  const { host, calls } = fixture();
  const media = createBrowserMediaDriver(() => host);
  const fetch = t.mock.method(
    globalThis,
    'fetch',
    async () =>
      new Response('abc', { headers: { 'content-length': String(MAX_SELECTED_BYTES + 1) } }),
  );
  await assert.rejects(
    media.sendFile({
      spaceId: 'sp',
      name: 'file.txt',
      mime: 'text/plain',
      uri: 'https://evil.test/file',
    }),
    /Selecione/,
  );
  await assert.rejects(
    media.sendFile({
      spaceId: 'sp',
      name: 'file.txt',
      mime: 'text/plain',
      uri: 'blob:https://another.test/id',
    }),
    /Selecione/,
  );
  assert.equal(fetch.mock.callCount(), 0);
  await assert.rejects(
    media.sendFile({
      spaceId: 'sp',
      name: 'file.txt',
      mime: 'text/plain',
      uri: 'blob:https://zoen.test/id',
    }),
    /64 MB/,
  );
  assert.equal(calls.length, 0);
});

test('voice sends actual recorder codec and keeps its thread target in the same signed file', async (t) => {
  browser(t);
  const { host, calls } = fixture();
  t.mock.method(
    globalThis,
    'fetch',
    async () => new Response('abc', { headers: { 'content-type': 'audio/mp4' } }),
  );
  const result = await createBrowserMediaDriver(() => host).sendVoice({
    spaceId: 'sp',
    uri: 'blob:https://zoen.test/clip',
    mime: 'audio/webm',
    durationMs: 1800,
    levels: [0, 0.5, 1],
    replyTo: 'root',
    thread: true,
  });
  assert.equal(result.itemId, 'it_signed');
  assert.equal(calls[0]?.[0], 'sp');
  assert.equal(calls[0]?.[1].mime, 'audio/mp4');
  assert.match(calls[0]?.[1].name ?? '', /\.m4a$/);
  assert.deepEqual(calls[0]?.[1].voice, {
    ms: 1800,
    levels: '08f',
    transcript: '',
    reply: { to: 'root', thread: true },
  });
  assert.deepEqual(Array.from(calls[0]?.[1].bytes ?? []), [97, 98, 99]);
});

test('preview reads pin historical version and revoke all decrypted URLs when account closes', async (t) => {
  const { host, controller } = fixture();
  const versions: (number | undefined)[] = [];
  const original = host.file;
  host.file = (itemId, version) => {
    versions.push(version);
    return original(itemId, version);
  };
  const revoked: string[] = [];
  t.mock.method(URL, 'revokeObjectURL', (uri: string) => {
    revoked.push(uri);
  });
  const media = createBrowserMediaDriver(() => host);
  const local = await media.materializeFile('it_signed', 2);
  assert.equal(local?.version, 2);
  assert.deepEqual(versions, [2]);
  assert.equal(local?.info.sha256, file.sha256);
  controller.abort();
  assert.deepEqual(revoked, [local?.uri]);
  await media.releaseLocalFile(local!.uri);
  assert.equal(revoked.length, 1);
});

test('a completed file read cannot expose decrypted bytes after its account was replaced', async (t) => {
  const { host, controller } = fixture();
  let finish: (blob: Blob) => void = () => {};
  const waiting = new Promise<Blob>((resolve) => {
    finish = resolve;
  });
  host.openFile = () => waiting;
  const created = t.mock.method(URL, 'createObjectURL', () => 'blob:must-not-exist');
  const media = createBrowserMediaDriver(() => host);
  const read = media.materializeFile('it_signed');
  await Promise.resolve();
  controller.abort();
  finish(new Blob(['abc']));
  await assert.rejects(read, /sessão mudou/);
  assert.equal(created.mock.callCount(), 0);
});
