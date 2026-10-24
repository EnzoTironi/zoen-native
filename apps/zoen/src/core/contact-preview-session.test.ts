import assert from 'node:assert/strict';
import test from 'node:test';
import { ContactPreviewSession } from '../media/contact-preview-session.ts';
import { isContactFileMime } from '../media/contact-file.ts';
import type { ContactPreview, LocalMediaFile, MediaDriver } from '../media/types.ts';
const contact: ContactPreview = {
  name: 'Ana Teste',
  phones: [{ label: 'mobile', value: '+55 31 1234 5678' }],
};
const file: LocalMediaFile = {
  uri: 'file:///synthetic/contact.vcf',
  itemId: 'contact-item',
  version: 2,
  info: {
    name: 'contact.vcf',
    path: '',
    mime: 'text/vcard',
    bytes: '80',
    sha256: 'a'.repeat(64),
    ready: true,
    chunks: 1,
    chunks_here: 1,
    has_thumbnail: false,
  },
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
const tick = async () => {
  for (let n = 0; n < 20; n++) await Promise.resolve();
};
function fixture() {
  let active = true,
    materializeGate: Promise<LocalMediaFile> | null = null,
    parseGate: Promise<ContactPreview> | null = null;
  const releases: string[] = [],
    reads: LocalMediaFile[] = [],
    requests: unknown[] = [];
  const media: MediaDriver = {
    sendFile: async () => assert.fail('preview never sends'),
    sendVoice: async () => assert.fail('preview never records'),
    fileInfo: async () => assert.fail('use observed binding'),
    materializeFile: async (itemId, version) => {
      requests.push({ itemId, version });
      return materializeGate ?? structuredClone(file);
    },
    readContact: async (value) => {
      reads.push(value);
      return parseGate ?? structuredClone(contact);
    },
    releaseLocalFile: async (uri) => {
      releases.push(uri);
    },
  };
  const session = new ContactPreviewSession({
    media,
    source: { itemId: file.itemId, version: 2, expectedHash: file.info.sha256 },
    current: () => active,
  });
  return {
    session,
    releases,
    reads,
    requests,
    retire: () => {
      active = false;
      return session.close();
    },
    materialize: (gate: Promise<LocalMediaFile>) => {
      materializeGate = gate;
    },
    parse: (gate: Promise<ContactPreview>) => {
      parseGate = gate;
    },
  };
}

test('contact bytes are deferred until explicit open; rapid taps share one exact authorized copy', async () => {
  const f = fixture();
  assert.deepEqual(f.requests, []);
  const gate = deferred<ContactPreview>();
  f.parse(gate.promise);
  const first = f.session.open(),
    second = f.session.open();
  await tick();
  assert.deepEqual(f.requests, [{ itemId: file.itemId, version: 2 }]);
  assert.equal(f.reads.length, 1);
  assert.deepEqual(f.releases, []);
  gate.resolve(contact);
  await Promise.all([first, second]);
  assert.deepEqual(f.session.getSnapshot().contact, contact);
  await f.session.dismiss(f.session.getSnapshot().ticket);
  assert.deepEqual(f.releases, [file.uri]);
  assert.equal(f.session.getSnapshot().visible, false);
  assert.deepEqual(f.session.getSnapshot().contact, contact);
  await f.retire();
  assert.equal(f.session.getSnapshot().contact, null);
});

test('cancel during materialization awaits cleanup and never parses a late copy', async () => {
  const f = fixture(),
    gate = deferred<LocalMediaFile>();
  f.materialize(gate.promise);
  const opening = f.session.open();
  await tick();
  let cleaned = false;
  const closing = f.session.close().then(() => {
    cleaned = true;
  });
  await tick();
  assert.equal(cleaned, false);
  gate.resolve(file);
  await Promise.all([opening, closing]);
  assert.equal(f.reads.length, 0);
  assert.deepEqual(f.releases, [file.uri]);
  assert.equal(f.session.getSnapshot().contact, null);
});

test('late native parse after owner retirement is dropped and released once', async () => {
  const f = fixture(),
    gate = deferred<ContactPreview>();
  f.parse(gate.promise);
  const opening = f.session.open();
  await tick();
  const closing = f.retire();
  gate.resolve(contact);
  await Promise.all([opening, closing]);
  assert.equal(f.session.getSnapshot().contact, null);
  assert.equal(f.session.getSnapshot().visible, false);
  assert.deepEqual(f.releases, [file.uri]);
});

test('foreign item/version/hash never reaches the native contact parser', async () => {
  for (const substitute of [
    { itemId: 'foreign' },
    { version: 3 },
    { info: { ...file.info, sha256: 'b'.repeat(64) } },
  ]) {
    const f = fixture();
    f.materialize(Promise.resolve({ ...file, ...substitute }));
    await f.session.open();
    assert.equal(f.reads.length, 0);
    assert.equal(f.session.getSnapshot().phase, 'error');
    assert.deepEqual(f.releases, [file.uri]);
  }
});

test('late dismissal and phone callback cannot act on a reopened preview of the same file', async () => {
  const f = fixture();
  await f.session.open();
  const old = f.session.getSnapshot();
  await f.session.dismiss(old.ticket);
  await f.session.open();
  await f.session.dismiss(old.ticket);
  assert.equal(f.session.getSnapshot().visible, true);
  let actions = 0;
  await f.session.perform(old.contact!, async () => {
    actions++;
  });
  assert.equal(actions, 0);
  await f.session.perform(f.session.getSnapshot().contact!, async () => {
    actions++;
  });
  assert.equal(actions, 1);
  await f.session.close();
});

test('retirement during OS readiness blocks dispatch and holds the file until the action ends', async () => {
  const f = fixture();
  await f.session.open();
  const gate = deferred<void>();
  let actions = 0,
    closed = false;
  const action = f.session.perform(f.session.getSnapshot().contact!, async (_file, current) => {
    await gate.promise;
    if (current()) actions++;
  });
  const closing = f.retire().then(() => {
    closed = true;
  });
  await tick();
  assert.equal(closed, false);
  assert.deepEqual(f.releases, []);
  gate.resolve();
  await Promise.all([action, closing]);
  assert.equal(actions, 0);
  assert.deepEqual(f.releases, [file.uri]);
});

test('explicit actions coalesce and cannot run after closing', async () => {
  const f = fixture();
  await f.session.open();
  const expected = f.session.getSnapshot().contact!,
    gate = deferred<void>();
  let actions = 0;
  const first = f.session.perform(expected, async () => {
    actions++;
    await gate.promise;
  });
  await f.session.perform(expected, async () => {
    actions++;
  });
  assert.equal(actions, 1);
  gate.resolve();
  await first;
  await f.session.close();
  await f.session.perform(expected, async () => {
    actions++;
  });
  assert.equal(actions, 1);
});

test('contact MIME routing ignores names and supports actual base MIME parameters', () => {
  for (const value of ['text/vcard', 'text/x-vcard', 'Text/VCard ; charset=utf-8'])
    assert.equal(isContactFileMime(value), true);
  for (const value of ['text/plain', 'contact.vcf', 'image/vcard', 'text/vcard-evil'])
    assert.equal(isContactFileMime(value), false);
});
