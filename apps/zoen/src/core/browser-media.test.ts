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
    ownerId: 'owner',
    deviceId: 'device',
    async saveProfile() {
      throw new Error('not used');
    },
    async profilePhoto() {
      throw new Error('not used');
    },
    async newAttachmentMutationId() {
      return 'attachment_fixture_1';
    },
    async releaseAttachmentMutation() {},
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
      replyTo: null,
      thread: false,
      mutationId: 'qa-file-selection',
      name: 'file.txt',
      mime: 'text/plain',
      uri: 'https://evil.test/file',
    }),
    /Selecione/,
  );
  await assert.rejects(
    media.sendFile({
      spaceId: 'sp',
      replyTo: null,
      thread: false,
      mutationId: 'qa-file-selection',
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
      replyTo: null,
      thread: false,
      mutationId: 'qa-file-selection',
      name: 'file.txt',
      mime: 'text/plain',
      uri: 'blob:https://zoen.test/id',
    }),
    /64 MB/,
  );
  assert.equal(calls.length, 0);
});

test('web attachments retain exact caption, mutation and reply/thread target instead of falling back', async (t) => {
  browser(t);
  const { host, calls } = fixture();
  const media = createBrowserMediaDriver(() => host);
  t.mock.method(globalThis, 'fetch', async () => new Response('abc'));
  for (const thread of [false, true]) {
    const selection = {
      spaceId: 'sp',
      replyTo: 'msg-root',
      thread,
      mutationId: 'attachment_fixture_1',
      caption: '  Página 2 🗂️\nKeep exact bytes.  ',
      name: 'file.txt',
      mime: 'text/plain',
      uri: 'blob:https://zoen.test/id',
    };
    await media.sendFile(selection);
    await media.sendFile(selection);
    const sent = calls.at(-1)!;
    assert.equal(sent[0], 'sp');
    assert.deepEqual(sent[1].attachment, {
      mutationId: selection.mutationId,
      caption: selection.caption,
      replyTo: selection.replyTo,
      thread,
    });
    assert.deepEqual(calls.at(-2), sent);
  }
});

test('browser producer ACK/drop stays bound to the issuing host across owner replacement', async () => {
  const first = fixture();
  const second = fixture();
  let host = first.host;
  const released: string[] = [];
  first.host.releaseAttachmentMutation = async (mutationId, accepted) => {
    released.push(`${mutationId}:${accepted}`);
  };
  second.host.releaseAttachmentMutation = async () => {
    assert.fail('old producer must not enter a new host');
  };
  const intents = createBrowserMediaDriver(() => host).attachmentIntents!;
  const mutation = await intents.newId();
  host = second.host;
  await intents.release(mutation, true);
  await intents.release(mutation, true);
  assert.deepEqual(released, [`${mutation}:true`]);
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

test('file ingress freezes caption and target before a delayed selected Blob read', async (t) => {
  browser(t);
  const { host, calls } = fixture();
  let finish!: (response: Response) => void;
  t.mock.method(
    globalThis,
    'fetch',
    () =>
      new Promise<Response>((resolve) => {
        finish = resolve;
      }),
  );
  const selection = {
    spaceId: 'sp',
    replyTo: 'root',
    thread: true,
    mutationId: 'attachment_fixture_1',
    caption: '  Original.  ',
    name: 'file.txt',
    mime: 'text/plain',
    uri: 'blob:https://zoen.test/id',
  };
  const sending = createBrowserMediaDriver(() => host).sendFile(selection);
  selection.caption = 'changed';
  selection.replyTo = 'another';
  selection.thread = false;
  finish(new Response('abc'));
  await sending;
  assert.deepEqual(calls[0]![1].attachment, {
    mutationId: 'attachment_fixture_1',
    caption: '  Original.  ',
    replyTo: 'root',
    thread: true,
  });
});
