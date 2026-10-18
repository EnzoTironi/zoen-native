import assert from 'node:assert/strict';
import { test } from 'node:test';
import { VoiceDraftOwner } from '../media/voice-draft-owner.ts';
import { MediaDraftChannel } from '../media/media-draft-channel.ts';
import { SensitiveCacheWork } from '../media/cache-work.ts';
import type {
  MediaDraftAsset,
  MediaDraftDto,
  MediaDraftPort,
  MediaDraftSendResult,
} from '../media/media-draft-types.ts';
import type { VoiceNativePort } from '../media/voice-session.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
const scope = {
  expected_identity_id: 'owner',
  expected_device_id: 'device',
  space_id: 'chat',
  thread_root_id: null,
};
const empty = (): MediaDraftDto => ({
  revision: '0',
  content_json: 'null',
  assets: [],
  attempted: false,
});
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value));

async function fixture(canonicalize = false) {
  const events: string[] = [];
  let value = empty(),
    revision = 0,
    sequence = 0,
    renders = 0,
    ids = 0,
    publications = 0;
  let failSave = false,
    failPrepare = false,
    loseSendAck = false;
  let saving: Promise<void> = Promise.resolve();
  let preparation: Promise<void> = Promise.resolve();
  const descriptors = new Map<string, MediaDraftAsset>();
  const accepted = new Map<string, MediaDraftSendResult>();
  const port: MediaDraftPort = {
    load: async () => clone(value),
    save: async (_scope, expected, content, assets) => {
      events.push('save-start');
      if (assets.length) events.push('source-ingest');
      await saving;
      if (failSave) throw new Error('synthetic storage failure');
      assert.equal(expected, value.revision);
      for (const asset of assets)
        descriptors.set(asset.id, {
          id: asset.id,
          name: asset.name,
          mime: asset.mime,
          size: 128,
          sha256: 'a'.repeat(64),
        });
      const parsed = JSON.parse(content);
      if (canonicalize && parsed) {
        // The real Rust codec normalizes opaque f64 JSON and stores render
        // peaks as f32; a successful ACK does not promise decimal identity.
        parsed.editor.draft.original.levels = parsed.editor.draft.original.levels.map(
          (level: number) => (level === 0.12515795230865479 ? 0.1251579523086548 : level),
        );
        if (parsed.render)
          parsed.render.levels = parsed.render.levels.map((level: number) =>
            level === 0.4000000059604645 ? 0.4 : level,
          );
      }
      const refs = parsed
        ? [parsed.source_asset_id, ...(parsed.render ? [parsed.render.asset_id] : [])]
        : [];
      value = {
        revision: `revision_${++revision}`,
        content_json: canonicalize ? JSON.stringify(parsed) : content,
        assets: refs.map((id) => descriptors.get(id)!),
        attempted: false,
      };
      events.push('save-accepted');
      return { accepted_revision: value.revision, draft: clone(value) };
    },
    materialize: async (_scope, expected, id) => {
      assert.equal(expected, value.revision);
      return { ...value.assets.find((asset) => asset.id === id)!, uri: `file:///owned/${id}.m4a` };
    },
    send: async (_scope, expected) => {
      events.push(`send:${expected}`);
      if (accepted.has(expected)) return { ...accepted.get(expected)!, draft: clone(value) };
      assert.equal(expected, value.revision);
      ++publications;
      value = { ...empty(), revision: `revision_${++revision}` };
      const result: MediaDraftSendResult = { entries: [], draft: clone(value) };
      accepted.set(expected, result);
      if (loseSendAck) {
        loseSendAck = false;
        throw new Error('synthetic lost acceptance');
      }
      return result;
    },
    release: async (uri) => {
      events.push(`materialized-release:${uri}`);
    },
  };
  const channel = new MediaDraftChannel(scope, port);
  await channel.load();
  const native: VoiceNativePort = {
    prepare: async () => {
      events.push('native-prepare-start');
      await preparation;
      if (failPrepare) throw new Error('synthetic native preparation failure');
      return {
        id: `native_${++sequence}`,
        uri: `file:///native/${sequence}/original.m4a`,
        sampleRate: canonicalize ? 24000 : 1000,
        frameCount: canonicalize ? 96000 : 4000,
        durationMs: 4000,
        levels: canonicalize ? Array(160).fill(0.12515795230865479) : [0.2, 0.8],
      };
    },
    render: async (_source, ranges) => ({
      uri: `file:///native/render_${++renders}.m4a`,
      mime: 'audio/mp4',
      durationMs:
        (ranges.reduce((sum, range) => sum + range.endFrame - range.startFrame, 0) /
          (canonicalize ? 24000 : 1000)) *
        1000,
      frameCount: ranges.reduce((sum, range) => sum + range.endFrame - range.startFrame, 0),
      levels: canonicalize ? [0.4000000059604645] : [0.4],
    }),
    transcribe: async () => ({ status: 'unavailable', text: '', words: [] }),
    releaseRender: async () => {},
    cancel: async () => {},
    release: async (id) => {
      events.push(`native-release:${id}`);
    },
  };
  const errors: unknown[] = [];
  const draft = new VoiceDraftOwner({
    channel,
    port: native,
    uri: 'file:///cache/capture.m4a',
    replyTo: 'quoted',
    work: new SensitiveCacheWork(),
    registerCleanup: () => () => {},
    discard: async () => {
      events.push('capture-discard');
    },
    onError: (error) => errors.push(error),
    id: () => `id_${++ids}`,
  });
  return {
    draft,
    channel,
    events,
    errors,
    get value() {
      return value;
    },
    get publications() {
      return publications;
    },
    delaySave: (promise: Promise<void>) => {
      saving = promise;
    },
    delayPrepare: (promise: Promise<void>) => {
      preparation = promise;
    },
    failSave: (failed: boolean) => {
      failSave = failed;
    },
    failPrepare: (failed: boolean) => {
      failPrepare = failed;
    },
    loseSendAck: () => {
      loseSendAck = true;
    },
  };
}

test('finalized voice ingestion gates Ready, retains only opaque editor metadata, and preserves its quote', async () => {
  const f = await fixture();
  const gate = deferred<void>();
  f.delaySave(gate.promise);
  const preparing = f.draft.prepare();
  for (let turn = 0; turn < 20 && !f.events.includes('save-start'); turn++) await Promise.resolve();
  assert.ok(f.events.includes('save-start'));
  assert.equal(f.draft.owner.session.getSnapshot().phase, 'preparing');
  assert.equal(f.draft.owner.session.getSendable(), null);
  gate.resolve();
  await preparing;
  const saved = JSON.parse(f.value.content_json);
  assert.equal(saved.reply_to, 'quoted');
  assert.equal(saved.source_asset_id, 'source_id_1');
  assert.equal(JSON.stringify(saved.editor).includes('file:'), false);
  assert.equal(f.draft.owner.session.getSnapshot().phase, 'ready');
  f.draft.retire();
  await f.channel.flush();
  assert.equal(
    JSON.parse(f.value.content_json).kind,
    'voice',
    'leaving the route preserves encrypted draft intent',
  );
});

test('canonical decimal normalization acknowledges the real voice owner before Ready and preserves later edits and send', async () => {
  const f = await fixture(true);
  await f.draft.prepare();
  const session = f.draft.owner.session;
  assert.equal(session.getSnapshot().phase, 'ready');
  assert.equal(session.getSnapshot().source!.sampleRate, 24000);
  assert.equal(session.getSnapshot().source!.levels.length, 160);
  assert.equal(session.getSnapshot().source!.levels[0], 0.12515795230865479);
  assert.equal(
    JSON.parse(f.value.content_json).editor.draft.original.levels[0],
    0.1251579523086548,
  );
  assert.equal(f.events.filter((event) => event === 'source-ingest').length, 1);

  session.edit({ type: 'trim', range: { startFrame: 2400, endFrame: 90000 } });
  await session.retryPreview();
  await f.channel.flush();
  assert.deepEqual(JSON.parse(f.value.content_json).editor.draft.edit.trim, {
    startFrame: 2400,
    endFrame: 90000,
  });
  assert.equal(f.events.filter((event) => event === 'source-ingest').length, 1);
  await f.draft.send(session.getSendable()!);
  assert.equal(f.publications, 1);
  assert.equal(f.value.content_json, 'null');
  assert.deepEqual(f.errors, []);
  await f.draft.owner.close();
});

test('failed source persistence keeps its actual native lease until retry accepts the same ingress', async () => {
  const f = await fixture();
  f.failSave(true);
  await assert.rejects(f.draft.prepare(), /prepare/);
  assert.equal(f.draft.owner.session.getSnapshot().phase, 'idle');
  assert.equal(f.draft.owner.session.getSendable(), null);
  f.draft.retire();
  await assert.rejects(f.channel.flush(), /storage failure/);
  assert.equal(
    f.events.some((event) => event.startsWith('native-release:')),
    false,
  );
  f.failSave(false);
  await f.channel.retry();
  assert.equal(f.value.assets.length, 1);
  assert.ok(f.events.includes('native-release:native_1'));
  assert.equal(f.events.filter((event) => event === 'capture-discard').length, 1);
});

test('pending precision range is durably distinct from the applied edit and cannot send old canonical audio', async () => {
  const f = await fixture();
  await f.draft.prepare();
  const session = f.draft.owner.session;
  const before = session.getSendable()!;
  session.setPendingTrim(session.getSnapshot().draft!, { startFrame: 100, endFrame: 3000 });
  await f.channel.flush();
  const saved = JSON.parse(f.value.content_json);
  assert.deepEqual(saved.editor.pendingTrim, { startFrame: 100, endFrame: 3000 });
  assert.deepEqual(saved.editor.draft.edit.trim, { startFrame: 0, endFrame: 4000 });
  assert.throws(() => session.undo());
  assert.throws(() => session.edit({ type: 'cut', range: { startFrame: 1000, endFrame: 2000 } }));
  await f.channel.flush();
  assert.deepEqual(JSON.parse(f.value.content_json).editor.pendingTrim, saved.editor.pendingTrim);
  await assert.rejects(f.draft.send(before));
  assert.equal(f.publications, 0);
  session.edit({ type: 'trim', range: { startFrame: 100, endFrame: 3000 } });
  await session.retryPreview();
  await f.channel.flush();
  assert.equal(JSON.parse(f.value.content_json).editor.pendingTrim, null);
  await f.draft.send(session.getSendable()!);
  assert.equal(f.publications, 1);
  assert.equal(f.value.content_json, 'null');
  await f.draft.owner.close();
});

test('an ambiguous accepted voice retries the exact frozen journal revision without republishing', async () => {
  const f = await fixture();
  await f.draft.prepare();
  const captured = f.draft.owner.session.getSendable()!;
  f.loseSendAck();
  await assert.rejects(f.draft.send(captured), /lost acceptance/);
  assert.equal(f.channel.getSnapshot().frozen, true);
  assert.equal(f.draft.owner.session.getSnapshot().attempted, true);
  assert.throws(() =>
    f.draft.owner.session.edit({ type: 'cut', range: { startFrame: 0, endFrame: 500 } }),
  );
  await f.draft.send(captured);
  assert.equal(f.publications, 1);
  assert.equal(new Set(f.events.filter((event) => event.startsWith('send:'))).size, 1);
  assert.equal(f.channel.getSnapshot().content, null);
  await f.draft.owner.close();
});

test('preserve retirement drains an admitted delayed native preparation before closing its source', async () => {
  const f = await fixture();
  const gate = deferred<void>();
  f.delaySave(gate.promise);
  const preparing = f.draft.prepare();
  f.draft.retire();
  const retiring = f.channel.retire(false);
  await Promise.resolve();
  assert.equal(
    f.events.some((event) => event.startsWith('native-release:')),
    false,
  );
  gate.resolve();
  await preparing;
  await retiring;
  assert.equal(JSON.parse(f.value.content_json).kind, 'voice');
  assert.ok(f.events.includes('native-release:native_1'));
});

test('native preparation failure before first save retains the finalized capture across a failed preserve retirement', async () => {
  const f = await fixture();
  f.failPrepare(true);
  await assert.rejects(f.draft.prepare(), /prepare/);
  assert.equal(f.value.content_json, 'null');
  assert.equal(f.events.includes('source-ingest'), false);
  f.draft.retire();
  await assert.rejects(f.channel.retire(false), /prepare/);
  assert.equal(f.events.includes('capture-discard'), false);
  f.failPrepare(false);
  await f.channel.retire(false);
  assert.equal(JSON.parse(f.value.content_json).kind, 'voice');
  assert.equal(f.events.filter((event) => event === 'capture-discard').length, 1);
  assert.ok(f.events.indexOf('save-accepted') < f.events.indexOf('capture-discard'));
});

test('explicit Cancel drains pending preparation and prevents its late source save before deleting the capture', async () => {
  const f = await fixture();
  const gate = deferred<void>();
  f.delayPrepare(gate.promise);
  const preparing = assert.rejects(f.draft.prepare(), /prepare/);
  for (let turn = 0; turn < 20 && !f.events.includes('native-prepare-start'); turn++)
    await Promise.resolve();
  assert.ok(f.events.includes('native-prepare-start'));
  const canceling = f.draft.discard();
  await Promise.resolve();
  assert.equal(f.events.includes('capture-discard'), false);
  gate.resolve();
  await preparing;
  await canceling;
  assert.equal(f.events.includes('source-ingest'), false);
  assert.equal(f.value.content_json, 'null');
  assert.equal(f.events.filter((event) => event === 'capture-discard').length, 1);
  await f.channel.retire(false);
});

test('Cancel after failed preparation removes its retained retry stage so later retirement cannot resurrect it', async () => {
  const f = await fixture();
  f.failPrepare(true);
  await assert.rejects(f.draft.prepare(), /prepare/);
  await f.draft.discard();
  await f.channel.retire(false);
  assert.equal(f.events.filter((event) => event === 'native-prepare-start').length, 1);
  assert.equal(f.events.filter((event) => event === 'capture-discard').length, 1);
  assert.equal(f.events.includes('source-ingest'), false);
  assert.equal(f.value.content_json, 'null');
});
