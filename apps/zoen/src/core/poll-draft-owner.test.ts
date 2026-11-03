import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PollDraftOwner } from '../media/poll-draft-owner.ts';
import { MediaDraftChannel } from '../media/media-draft-channel.ts';
import { contentValue } from '../media/media-draft-contract.ts';
import { fixture, deferred, tick, scope } from './media-draft-channel.fixtures.ts';
import type { MediaReplyTarget } from '../media/media-reply.ts';

const reply: MediaReplyTarget = {
  reply_to: 'source',
  reply_intent: 'A',
  reply_quote: {
    source_id: 'source',
    revision: 'r1',
    text: '😀',
    spans: [{ start_utf16: 6, end_utf16: 8 }],
  },
};
function own(f = fixture(), context = reply) {
  let current = true,
    sequence = 0;
  const owner = new PollDraftOwner(
    f.channel,
    context,
    () => current,
    () => `option_${++sequence}`,
  );
  owner.activate();
  return {
    f,
    owner,
    retire: () => {
      current = false;
    },
    reactivate: () => {
      current = true;
    },
  };
}
async function ready(value = own()) {
  await value.f.channel.load();
  await value.owner.setQuestion('Question?');
  await value.owner.setOption(value.owner.initial.options[0]!.id, 'First');
  await value.owner.setOption(value.owner.initial.options[1]!.id, 'Second');
  return value;
}

test('poll render/activation/load never publishes or creates a private draft; real edits are owned and immutable', async () => {
  const { f, owner } = own();
  assert.equal(f.calls.length, 0);
  assert.throws(() => owner.setQuestion('before load'));
  await f.channel.load();
  assert.equal(f.channel.getSnapshot().content, null);
  assert.equal(f.calls.filter((call) => call.op !== 'load').length, 0);
  await owner.setQuestion('');
  const poll = f.channel.getSnapshot().content;
  assert.equal(poll?.kind, 'poll');
  assert.equal(poll?.reply_to, 'source');
  assert.deepEqual(poll?.reply_quote, reply.reply_quote);
  assert.ok(Object.isFrozen(poll));
  assert.ok(Object.isFrozen(poll?.options));
  assert.throws(() => owner.submit(), /poll-invalid/);
  assert.equal(f.calls.filter((call) => call.op === 'send').length, 0);
});

test('rapid edits merge against the channel and stable options cannot retarget a removed field', async () => {
  const { f, owner } = await ready();
  await owner.addOption();
  const third = f.channel.getSnapshot().content;
  assert.equal(third?.kind, 'poll');
  if (third?.kind !== 'poll') throw Error('poll');
  const original = third.options[0]!.id,
    second = third.options[1]!.id,
    last = third.options[2]!.id;
  const gate = deferred();
  f.hooks.save = () => gate.promise;
  const a = owner.setQuestion('Latest question');
  const b = owner.setOption(second, 'Latest second');
  const c = owner.removeOption(original);
  const d = owner.setOption(last, 'Latest third');
  assert.throws(() => owner.setOption(original, 'late deleted-field callback'));
  gate.resolve();
  await Promise.all([a, b, c, d]);
  assert.deepEqual(f.channel.getSnapshot().content, {
    ...third,
    question: 'Latest question',
    options: [
      { id: second, text: 'Latest second' },
      { id: last, text: 'Latest third' },
    ],
  });
  assert.equal(f.calls.filter((call) => call.op === 'send').length, 0);
});

test('Send seals before pending saves drain; double tap shares one intent and cannot edit the payload', async () => {
  const { f, owner } = await ready();
  const gate = deferred();
  f.hooks.save = () => gate.promise;
  const edit = owner.setQuestion('Final native question');
  const a = owner.submit(),
    b = owner.submit();
  assert.equal(a, b);
  assert.throws(() => owner.setQuestion('late'));
  gate.resolve();
  await edit;
  const result = await a;
  assert.equal(result.entries.length, 1);
  assert.equal(f.copies(), 1);
  assert.throws(() => owner.submit());
  assert.equal(f.calls.filter((call) => call.op === 'send').length, 1);
});

test('ambiguous acceptance freezes the same revision; an explicit retry recovers one poll', async () => {
  const { f, owner } = await ready();
  f.hooks.sent = async () => {
    throw Error('lost ACK');
  };
  await assert.rejects(owner.submit(), /lost ACK/);
  assert.equal(f.channel.getSnapshot().frozen, true);
  assert.throws(() => owner.setOption(owner.initial.options[0]!.id, 'cannot replace intent'));
  f.hooks.sent = undefined;
  await owner.submit();
  const attempts = f.calls.filter((call) => call.op === 'send');
  assert.equal(attempts.length, 2);
  assert.equal(attempts[0]!.revision, attempts[1]!.revision);
  assert.equal(f.copies(), 1);
});

test('a failed private save retains edits and retry repairs custody without publishing', async () => {
  const { f, owner } = await ready();
  f.hooks.save = async () => {
    throw Error('disk');
  };
  await assert.rejects(owner.setQuestion('Keep my question'), /disk/);
  const pending = f.channel.getSnapshot().content;
  assert.equal(pending?.kind === 'poll' && pending.question, 'Keep my question');
  f.hooks.save = undefined;
  await f.channel.retry();
  assert.deepEqual(JSON.parse(f.current().content_json), pending);
  assert.equal(f.copies(), 0);
});

test('cold reopening restores private fields and reply instead of adopting the new composer target', async () => {
  const { f, owner } = await ready();
  owner.retire();
  const cold = new MediaDraftChannel(scope, f.port);
  const next = new PollDraftOwner(
    cold,
    { reply_to: 'newer', reply_intent: 'B' },
    () => true,
    (() => {
      let n = 0;
      return () => `cold_${++n}`;
    })(),
  );
  next.activate();
  await cold.load();
  const prior = JSON.parse(f.current().content_json);
  assert.deepEqual(cold.getSnapshot().content, prior);
  await next.setQuestion('Cold edit');
  const changed = cold.getSnapshot().content;
  assert.equal(changed?.reply_to, 'source');
  assert.equal(changed?.reply_intent, 'A');
  assert.equal(f.copies(), 0);
});

test('unlink preserves latest fields and exposes exactly the captured normal reply to the matching caller', async () => {
  const { f, owner } = await ready();
  const gate = deferred();
  f.hooks.save = () => gate.promise;
  const edit = owner.setQuestion('My latest field');
  const cleared = owner.clearReply()!;
  assert.deepEqual(cleared.previous, reply);
  const pending = f.channel.getSnapshot().content;
  assert.equal(pending?.kind === 'poll' && pending.question, 'My latest field');
  assert.equal(pending?.reply_to, null);
  assert.equal(pending?.reply_quote, undefined);
  assert.equal(pending?.reply_intent, undefined);
  gate.resolve();
  await edit;
  await cleared.saved;
  assert.equal(JSON.parse(f.current().content_json).question, 'My latest field');
});

test('retired, background or replaced owners cannot edit, send, unlink or discard retained work', async () => {
  const o = await ready();
  const before = o.f.current();
  for (const retire of [() => o.owner.retire(), o.retire]) {
    retire();
    for (const action of [
      () => o.owner.setQuestion('stale'),
      () => o.owner.addOption(),
      () => o.owner.clearReply(),
      () => o.owner.submit(),
    ])
      assert.throws(action);
    await assert.rejects(o.owner.discard());
    assert.equal(o.f.current(), before);
    o.reactivate();
    o.owner.activate();
  }
  await o.owner.setQuestion('Strict Mode reactivation keeps the same journal');
  assert.equal(o.f.copies(), 0);
});

test('another pending attachment cannot be overwritten by a Poll route', async () => {
  const { f, owner } = await ready();
  await f.channel.clear();
  await f.channel.save(
    {
      kind: 'attachments',
      reply_to: null,
      items: [{ asset_id: 'a', caption: 'keep', mutation_id: 'post_a' }],
    },
    [{ id: 'a', uri: 'file:///a', name: 'a.jpg', mime: 'image/jpeg' }],
  );
  const before = f.current();
  assert.throws(() => owner.setQuestion('overwrite'));
  assert.throws(() => owner.submit());
  assert.equal(f.current(), before);
  await tick();
});

test('poll contract rejects unknown voting state, duplicate IDs, byte overflows and borrowed assets while keeping incomplete work', () => {
  const base = {
    kind: 'poll',
    reply_to: null,
    question: '',
    options: [
      { id: 'a', text: '' },
      { id: 'b', text: '' },
    ],
  };
  assert.deepEqual(contentValue(base).refs, []);
  for (const candidate of [
    { ...base, votes: [] },
    {
      ...base,
      options: [
        { id: 'a', text: 'one' },
        { id: 'a', text: 'two' },
      ],
    },
    { ...base, question: '😀'.repeat(76) },
    {
      ...base,
      options: [
        { id: 'a', text: '😀'.repeat(51) },
        { id: 'b', text: '' },
      ],
    },
    {
      ...base,
      options: [
        { id: 'a', text: 'one', votes: ['fake'] },
        { id: 'b', text: 'two' },
      ],
    },
    { ...base, assets: [] },
  ])
    assert.throws(() => contentValue(candidate));
});
