import assert from 'node:assert/strict';
import { test } from 'node:test';
import { stageLocationAttachment } from '../media/attachment-route-actions.ts';
import { MediaDraftChannel } from '../media/media-draft-channel.ts';
import type { MediaDraftDto, MediaDraftPort } from '../media/media-draft-types.ts';
import { parseLocationGeoJSON } from '../media/location-file.ts';
const scope = {
  expected_identity_id: 'owner',
  expected_device_id: 'phone',
  space_id: 'space',
  thread_root_id: 'thread',
};
function fixture(lost = false) {
  let stored: MediaDraftDto = { revision: '0', content_json: 'null', assets: [], attempted: false };
  let result: { accepted_revision: string; draft: MediaDraftDto } | null = null;
  let writes = 0,
    sends = 0,
    prepared = 0,
    released = 0;
  const bytes: string[] = [];
  const port: MediaDraftPort = {
    load: async () => stored,
    save: async (_, revision, json, assets) => {
      if (result && revision === '0') return result;
      assert.equal(revision, stored.revision);
      writes++;
      stored = {
        revision: String(writes),
        content_json: json,
        assets: assets.map((a) => ({
          id: a.id,
          name: a.name,
          mime: a.mime,
          sha256: 'a'.repeat(64),
          size: bytes[0]!.length,
        })),
        attempted: false,
      };
      result = { accepted_revision: stored.revision, draft: stored };
      if (lost) {
        lost = false;
        throw Error('ack-lost');
      }
      return result;
    },
    send: async () => {
      sends++;
      throw Error('staging must never send');
    },
    materialize: async () => {
      throw Error('not needed');
    },
    release: async () => {},
  };
  const channel = new MediaDraftChannel(scope, port);
  const stage = (current = () => true, caption = 'Reviewed 👩‍💻') =>
    stageLocationAttachment({
      media: channel,
      point: { longitude: -43, latitude: -19, label: 'A place' },
      caption,
      current,
      replyTo: () => 'reply',
      nextId: () => 'location-intent',
      admitted: () => {},
      prepare: async (text) => {
        prepared++;
        bytes.push(text);
        return {
          uri: 'file:///location.geojson',
          release: async () => {
            released++;
          },
        };
      },
    });
  return {
    channel,
    stage,
    cold: () => new MediaDraftChannel(scope, port),
    bytes,
    counts: () => ({ writes, sends, prepared, released }),
  };
}

test('explicit Add retains one exact GeoJSON/caption/reply/thread across lost ACK and cold journal reopen without send', async () => {
  const f = fixture(true);
  await assert.rejects(f.stage());
  assert.deepEqual(f.counts(), { writes: 1, sends: 0, prepared: 1, released: 0 });
  await f.channel.flush();
  assert.equal(f.counts().prepared, 1);
  assert.equal(f.counts().writes, 1);
  const cold = f.cold();
  await cold.load();
  const snapshot = cold.getSnapshot();
  assert.equal(snapshot.content?.kind, 'attachments');
  assert.equal(snapshot.content?.reply_to, 'reply');
  assert.deepEqual(snapshot.content?.items, [
    { asset_id: 'location-intent', mutation_id: 'location-intent', caption: 'Reviewed 👩‍💻' },
  ]);
  assert.deepEqual(parseLocationGeoJSON(f.bytes[0]!), {
    longitude: -43,
    latitude: -19,
    label: 'A place',
  });
  assert.equal(snapshot.value!.assets[0]!.mime, 'application/geo+json');
  assert.deepEqual(cold.scope, scope);
  assert.equal(f.counts().sends, 0);
});

test('Location Add cannot replace a pre-existing media draft or stage after owner retirement', async () => {
  const f = fixture();
  await f.stage();
  const before = f.channel.getSnapshot().value;
  await assert.rejects(f.stage(), /media-busy/);
  assert.equal(f.channel.getSnapshot().value, before);
  assert.equal(f.counts().prepared, 1);
  const retired = fixture();
  await assert.rejects(
    retired.stage(() => false),
    /retired/,
  );
  assert.equal(retired.counts().prepared, 0);
  assert.equal(retired.counts().writes, 0);
});

test('invalid location captions are refused before creating a cache copy or an admitted retry intent', async () => {
  const f = fixture();
  await assert.rejects(f.stage(() => true, 'a'.repeat(16385)));
  assert.equal(f.counts().prepared, 0);
  assert.equal(f.counts().writes, 0);
});
