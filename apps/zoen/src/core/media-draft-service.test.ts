import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from './store.ts';
import type { Command, CommandResult, CoreDriver, CoreEvent, Snapshot } from './types.ts';
import type { MediaDriver } from '../media/types.ts';
import type { MediaDraftDto, MediaDraftPort } from '../media/media-draft-types.ts';

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
const blank: Snapshot = {
  account: null,
  me: null,
  connection: { state: 'offline', synced: false, pending: '0', error: null },
  conversations: [],
  integrity_warnings: {},
  has_unreadable_local_history: false,
  requests: [],
  agents: [],
};
const account = (identity = 'owner', device = 'device'): Snapshot => ({
  ...blank,
  account: {
    identity_id: identity,
    device_id: device,
    name: 'Owner',
    handle: 'owner',
    relay_url: 'https://relay.example',
    registered: true,
    unlocked: true,
    setup_pending: false,
  },
  conversations: [{ id: 'chat', members: [{ id: identity }] }] as Snapshot['conversations'],
});
const content = {
  kind: 'attachments',
  reply_to: null,
  items: [{ asset_id: 'image', caption: 'private', mutation_id: 'post_image' }],
} as const;
const ingest = { id: 'image', uri: 'file:///image', name: 'image.jpg', mime: 'image/jpeg' };
const empty = (): MediaDraftDto => ({
  revision: '0',
  content_json: 'null',
  assets: [],
  attempted: false,
});
class Driver implements CoreDriver {
  media?: MediaDriver;
  snapshot = account();
  readonly events: string[] = [];
  saved = empty();
  beforeSave: (() => Promise<void>) | undefined;
  failSave = false;
  constructor(withPort = true) {
    const port: MediaDraftPort = {
      load: async () => {
        this.events.push('load');
        return this.saved;
      },
      save: async (_scope, expected, json) => {
        this.events.push('saving');
        await this.beforeSave?.();
        if (this.failSave) throw new Error('Native storage unavailable');
        assert.equal(expected, this.saved.revision);
        this.saved = {
          revision: `r${Number(this.saved.revision.slice(1) || '0') + 1}`,
          content_json: json,
          attempted: false,
          assets:
            json === 'null'
              ? []
              : [
                  {
                    id: 'image',
                    name: 'image.jpg',
                    mime: 'image/jpeg',
                    size: 3,
                    sha256: 'a'.repeat(64),
                  },
                ],
        };
        this.events.push('saved');
        return { accepted_revision: this.saved.revision, draft: this.saved };
      },
      send: async () => {
        throw new Error('Test never posts');
      },
      materialize: async () => {
        throw new Error('Test never materializes');
      },
      release: async () => {
        this.events.push('release');
      },
    };
    if (withPort) this.media = { drafts: port } as MediaDriver;
  }
  async open() {
    this.events.push('open');
    return this.snapshot;
  }
  async close() {
    this.events.push('close');
  }
  async eraseDevice() {
    this.events.push('erase');
    this.saved = empty();
    this.snapshot = blank;
  }
  async createAccount() {
    return this.snapshot;
  }
  subscribe(_listener: (event: CoreEvent) => void) {
    return () => {};
  }
  async request<C extends Command>(_command: C): Promise<CommandResult<C>> {
    return this.snapshot as CommandResult<C>;
  }
}

test('store channel lookup has no native IO, scopes account/device/thread, and refuses nonnative/demo/nonmembers', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  assert.equal(store.mediaDrafts.channel('chat', null), null);
  await store.open();
  const count = driver.events.length;
  const main = store.mediaDrafts.channel('chat', null)!;
  const thread = store.mediaDrafts.channel('chat', 'thread')!;
  assert.ok(main);
  assert.notEqual(main, thread);
  assert.equal(main, store.mediaDrafts.channel('chat', null));
  assert.equal(driver.events.length, count);
  assert.equal(store.mediaDrafts.channel('foreign-chat', null), null);
  assert.equal(store.mediaDrafts.isCurrent(main), true);
  await store.open('demo');
  assert.equal(store.mediaDrafts.channel('chat', null), null);
  assert.equal(store.mediaDrafts.isCurrent(main), false);
  driver.snapshot = account('second', 'next-device');
  await store.open();
  const next = store.mediaDrafts.channel('chat', null)!;
  assert.notEqual(next, main);
  assert.equal(next.scope.expected_identity_id, 'second');
  assert.equal(next.scope.expected_device_id, 'next-device');
  await store.close();
  const web = new ClientStore(new Driver(false));
  await web.open();
  assert.equal(web.mediaDrafts.channel('chat', null), null);
  await web.close();
});

test('actual store recovery service uses both scoped channels and never invokes Send', async () => {
  const driver = new Driver(),
    store = new ClientStore(driver);
  await store.open();
  const requests: unknown[] = [];
  driver.media!.drafts!.recoverFailed = async (scope, messageId, mediaRevision, normalRevision) => {
    requests.push([scope, messageId, mediaRevision, normalRevision]);
    return (
      await driver.media!.drafts!.save(scope, mediaRevision, JSON.stringify(content), [ingest])
    ).draft;
  };
  await store.mediaDrafts.recoverFailed('chat', null, 'failed', () => true);
  assert.deepEqual(requests, [
    [
      {
        expected_identity_id: 'owner',
        expected_device_id: 'device',
        space_id: 'chat',
        thread_root_id: null,
      },
      'failed',
      '0',
      '0',
    ],
  ]);
  assert.equal(store.mediaDrafts.channel('chat', null)!.getSnapshot().content?.kind, 'attachments');
  await store.close();
});

test('actual store recovery service preserves concurrent normal typing and rejects a late composer adoption', async () => {
  const driver = new Driver(),
    store = new ClientStore(driver),
    entered = deferred(),
    finished = deferred();
  await store.open();
  driver.media!.drafts!.recoverFailed = async (scope, _message, revision) => {
    entered.resolve();
    await finished.promise;
    return (await driver.media!.drafts!.save(scope, revision, JSON.stringify(content), [ingest]))
      .draft;
  };
  const pending = store.mediaDrafts.recoverFailed('chat', null, 'failed', () => true);
  await entered.promise;
  const normal = store.messageDrafts.channel('chat', null)!;
  normal.controller.setText(normal.scope, 'Keep concurrent text');
  finished.resolve();
  await assert.rejects(pending);
  assert.equal(normal.controller.getSnapshot(normal.scope).text, 'Keep concurrent text');
  assert.equal(store.mediaDrafts.channel('chat', null)!.getSnapshot().content, null);
  await store.close();
});

test('actual Store.close waits admitted ingress and durable CAS before closing the native engine', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const channel = store.mediaDrafts.channel('chat', null)!;
  await channel.load();
  const gate = deferred();
  const entered = deferred();
  const stage = channel.stage(async (save) => {
    entered.resolve();
    await gate.promise;
    await save(content, [ingest]);
  });
  await entered.promise;
  channel.deferRelease(async () => {
    driver.events.push('owner-released');
  });
  const previousCloses = driver.events.filter((event) => event === 'close').length;
  const closing = store.close();
  await tick();
  assert.equal(driver.events.filter((event) => event === 'close').length, previousCloses);
  assert.equal(store.mediaDrafts.channel('chat', null), null);
  gate.resolve();
  await Promise.all([stage, closing]);
  assert.ok(driver.events.indexOf('saved') < driver.events.indexOf('owner-released'));
  assert.ok(driver.events.indexOf('owner-released') < driver.events.lastIndexOf('close'));
  assert.notEqual(driver.saved.content_json, 'null');
  await assert.rejects(channel.save(content, [ingest]));
});

test('failed preserve retirement retains old owner and retries failed preparation before a new account opens', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const channel = store.mediaDrafts.channel('chat', null)!;
  await channel.load();
  let fail = true;
  await assert.rejects(
    channel.stage(async (save) => {
      if (fail) throw new Error('Prepare unavailable');
      await save(content, [ingest]);
    }),
    /Prepare unavailable/,
  );
  driver.snapshot = account('new-owner', 'new-device');
  const previous = store.getSnapshot();
  const epoch = store.sessionEpoch;
  const closes = driver.events.filter((event) => event === 'close').length;
  await store.open();
  assert.equal(store.getSnapshot().phase, 'ready');
  assert.equal(store.getSnapshot().snapshot, previous.snapshot);
  assert.equal(store.sessionEpoch, epoch);
  assert.equal(channel.scope.expected_identity_id, 'owner');
  assert.equal(driver.events.filter((event) => event === 'close').length, closes);
  assert.equal(store.mediaDrafts.channel('chat', null), null);
  fail = false;
  await store.open();
  assert.equal(store.getSnapshot().phase, 'ready');
  assert.equal(store.mediaDrafts.channel('chat', null)?.scope.expected_identity_id, 'new-owner');
  assert.notEqual(driver.saved.content_json, 'null');
  await store.close();
});

test('actual erase cancels queued media writes, drains admitted native IO, then releases owners before erasure', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const channel = store.mediaDrafts.channel('chat', null)!;
  await channel.load();
  const entered = deferred();
  const gate = deferred();
  driver.beforeSave = async () => {
    entered.resolve();
    await gate.promise;
  };
  const first = channel.save(content, [ingest]);
  const firstRejected = assert.rejects(first);
  await entered.promise;
  const second = channel.save({ ...content, items: [{ ...content.items[0], caption: 'newer' }] });
  const secondRejected = assert.rejects(second);
  channel.deferRelease(async () => {
    driver.events.push('owner-released');
  });
  const erasing = store.eraseDevice({ identity_id: 'owner', device_id: 'device' });
  await tick();
  assert.equal(driver.events.includes('erase'), false);
  gate.resolve();
  await Promise.all([firstRejected, secondRejected, erasing]);
  assert.equal(driver.events.filter((event) => event === 'saving').length, 1);
  assert.ok(driver.events.indexOf('saved') < driver.events.indexOf('owner-released'));
  assert.ok(driver.events.indexOf('owner-released') < driver.events.indexOf('erase'));
  assert.equal(driver.saved.content_json, 'null');
  await store.close();
});
