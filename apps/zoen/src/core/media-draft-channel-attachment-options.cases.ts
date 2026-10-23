import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MediaDraftChannel } from '../media/media-draft-channel.ts';
import type { MediaDraftContent } from '../media/media-draft-types.ts';
import {
  content,
  fixture,
  ingest,
  kind,
  multipleContent,
  scope,
} from './media-draft-channel.fixtures.ts';

test('metadata validates closed unions and rejects native URIs or SDK objects in editor JSON', async () => {
  const f = fixture();
  await f.channel.load();
  const source = { ...ingest(), name: 'a.m4a', mime: 'audio/mp4' };
  const voice = {
    kind: 'voice' as const,
    reply_to: null,
    source_asset_id: 'a',
    editor: { selection: [0, 1] },
    render: null,
  };
  for (const editor of [
    { uri: 'file:///secret' },
    { original: { data: '/private/cache/secret' } },
    new Date(),
    { bad: NaN },
  ])
    await assert.rejects(f.channel.save({ ...voice, editor }, [source]), kind('invalid'));
  await assert.rejects(
    f.channel.save({ ...content(), unknown: true } as unknown as MediaDraftContent, [ingest()]),
    kind('invalid'),
  );
  await f.channel.save(voice, [source]);
  assert.equal(f.current().assets[0]?.mime, 'audio/mp4');
  assert.ok(Object.isFrozen(f.channel.getSnapshot().content));
});

test('attachment defaults preserve legacy content JSON on save and restored native projection', async () => {
  const f = fixture();
  await f.channel.load();
  const legacy = content();
  const explicit = {
    ...legacy,
    options: { presentation: 'media' as const, grouping: 'separate' as const },
  };
  await f.channel.save(explicit, [ingest()]);
  assert.equal(f.calls.find((call) => call.op === 'save')?.json, JSON.stringify(legacy));
  assert.equal(f.current().content_json, JSON.stringify(legacy));
  assert.deepEqual(f.channel.getSnapshot().content, legacy);

  f.set({ ...f.current(), content_json: JSON.stringify(explicit) });
  const restored = new MediaDraftChannel(scope, f.port);
  await restored.load();
  assert.equal(restored.getSnapshot().value?.content_json, JSON.stringify(legacy));
  assert.deepEqual(restored.getSnapshot().content, legacy);
});

test('attachment nondefault presentation and grouping persist in the same canonical content', async () => {
  for (const options of [
    { presentation: 'file', grouping: 'separate' },
    { presentation: 'media', grouping: 'grouped' },
    { presentation: 'file', grouping: 'grouped' },
  ] as const) {
    const f = fixture();
    await f.channel.load();
    const selected = { ...multipleContent(), options };
    await f.channel.save(selected, [ingest(), ingest('b')]);
    assert.equal(f.current().content_json, JSON.stringify(selected));
    const projected = f.channel.getSnapshot().content;
    assert.ok(projected?.kind === 'attachments');
    assert.deepEqual(projected.options, options);
    assert.ok(Object.isFrozen(projected.options));

    const restored = new MediaDraftChannel(scope, f.port);
    await restored.load();
    assert.deepEqual(restored.getSnapshot().content, selected);
  }
});

test('attachment options reject malformed, partial, unsupported and quality fields before any native save', async () => {
  const f = fixture();
  await f.channel.load();
  for (const options of [
    null,
    undefined,
    false,
    [],
    {},
    { presentation: 'file' },
    { grouping: 'grouped' },
    { presentation: 'document', grouping: 'separate' },
    { presentation: 'media', grouping: 'album' },
    { presentation: 'media', grouping: 'separate', quality: 'original' },
  ])
    await assert.rejects(
      f.channel.save({ ...multipleContent(), options } as unknown as MediaDraftContent, [
        ingest(),
        ingest('b'),
      ]),
      kind('invalid'),
    );
  await assert.rejects(
    f.channel.save({ ...multipleContent(), quality: 'original' } as unknown as MediaDraftContent, [
      ingest(),
      ingest('b'),
    ]),
    kind('invalid'),
  );
  assert.equal(f.calls.filter((call) => call.op === 'save').length, 0);
  assert.equal(f.channel.getSnapshot().content, null);
});

test('grouped attachments accept exactly two through ten assets and reject singletons or overflow', async () => {
  const options = { presentation: 'media' as const, grouping: 'grouped' as const };
  for (const count of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]) {
    const f = fixture();
    await f.channel.load();
    const selected = { ...multipleContent(count), options };
    const inputs = selected.items.map((item) => {
      const input = ingest(item.asset_id);
      f.sources.set(input.uri, `bytes-${input.id}`);
      return input;
    });
    if (count === 1 || count === 11) {
      await assert.rejects(f.channel.save(selected, inputs), kind('invalid'));
      assert.equal(f.calls.filter((call) => call.op === 'save').length, 0);
    } else {
      await f.channel.save(selected, inputs);
      assert.deepEqual(f.channel.getSnapshot().content, selected);
      assert.equal(f.current().assets.length, count);
    }
  }
});

test('ambiguous send retains immutable attachment options and retries its original revision', async () => {
  const f = fixture();
  await f.channel.load();
  const options: {
    presentation: 'media' | 'file';
    grouping: 'separate' | 'grouped';
  } = { presentation: 'file', grouping: 'grouped' };
  const selected = { ...multipleContent(), options };
  await f.channel.save(selected, [ingest(), ingest('b')]);
  const canonical = f.current();
  f.hooks.send = async () => {
    throw new Error('Unknown acceptance');
  };
  await assert.rejects(f.channel.send(), /Unknown acceptance/);
  options.presentation = 'media';
  options.grouping = 'separate';
  assert.equal(f.channel.getSnapshot().frozen, true);
  const projected = f.channel.getSnapshot().content;
  assert.ok(projected?.kind === 'attachments');
  assert.deepEqual(projected.options, { presentation: 'file', grouping: 'grouped' });
  assert.deepEqual(f.current(), canonical);
  await assert.rejects(f.channel.save(selected), kind('attempted'));
  assert.equal(f.calls.filter((call) => call.op === 'save').length, 1);

  f.hooks.send = undefined;
  await f.channel.retry();
  assert.deepEqual(
    f.calls.filter((call) => call.op === 'send').map((call) => call.revision),
    [canonical.revision, canonical.revision],
  );
  assert.equal(f.copies(), 1);
  assert.equal(f.channel.getSnapshot().content, null);
  assert.equal(f.channel.getSnapshot().frozen, false);
});
