import assert from 'node:assert/strict';
import { test } from 'node:test';
import { contentValue } from '../media/media-draft-contract.ts';
import { mediaAcceptedIntent } from '../media/media-acceptance.ts';
import { captureMediaReply } from '../media/media-reply.ts';
import { canConsumeAcceptedMediaReply } from './message-draft.ts';
import { fixture, content, ingest, empty, tick } from './media-draft-channel.fixtures.ts';
import { messageDraftReactFixture } from './message-draft-react.fixtures.ts';
import { fixture as attachmentFixture, photo } from './durable-attachment-composer.fixtures.ts';
import type { TimelineEntry } from './types.ts';

const quote = () => ({
  source_id: 'source',
  revision: 'source_r1',
  text: '😀',
  spans: [{ start_utf16: 6, end_utf16: 8 }],
});
const shared = () => ({
  space_id: 'private',
  message_id: 'source',
  revision: 'source_r1',
  author_id: 'author',
  author_name: 'Author',
  text: '😀',
  selection: quote(),
});

test('media codec owns reviewed context, retains legacy bytes, and refuses malformed context', () => {
  const legacy = content();
  assert.equal(contentValue(legacy).json, JSON.stringify(legacy));
  for (const kind of ['local', 'shared'] as const) {
    const selection = quote(),
      cross = shared();
    const input = {
      ...legacy,
      reply_to: kind === 'local' ? 'source' : null,
      reply_intent: 'intent_A',
      ...(kind === 'local' ? { reply_quote: selection } : { shared_reply: cross }),
    };
    const parsed = contentValue(input).content!;
    selection.text = 'mutated';
    cross.author_name = 'mutated';
    cross.selection.spans[0]!.end_utf16 = 99;
    assert.equal(parsed.reply_intent, 'intent_A');
    assert.equal(
      kind === 'local' ? parsed.reply_quote?.text : parsed.shared_reply?.selection?.text,
      '😀',
    );
    assert.ok(
      Object.isFrozen(
        kind === 'local' ? parsed.reply_quote?.spans[0] : parsed.shared_reply?.selection?.spans[0],
      ),
    );
    assert.deepEqual(contentValue(JSON.parse(contentValue(parsed).json)).content, parsed);
  }
  for (const invalid of [
    { ...legacy, reply_intent: '0' },
    { ...legacy, reply_intent: 'orphan' },
    { ...legacy, reply_to: 'different', reply_quote: quote() },
    { ...legacy, reply_to: 'source', reply_quote: quote(), shared_reply: shared() },
    { ...legacy, shared_reply: { ...shared(), selection: { ...quote(), source_id: 'forged' } } },
  ])
    assert.throws(() => contentValue(invalid));
});

test('acceptance cleanup matches reviewed context and its generation, preserving reselection and editing', () => {
  const accepted = {
    reply_to: 'source',
    ...captureMediaReply({ replyQuote: quote(), replyIntent: 'A' }),
  };
  assert.equal(canConsumeAcceptedMediaReply({ ...accepted, editing: false }, accepted), true);
  assert.equal(
    canConsumeAcceptedMediaReply(
      { ...accepted, editing: false },
      {
        ...accepted,
        reply_quote: {
          text: '😀',
          spans: [{ end_utf16: 8, start_utf16: 6 }],
          revision: 'source_r1',
          source_id: 'source',
        },
      },
    ),
    true,
    'Core serialization key order cannot change reply identity',
  );
  for (const change of [
    { reply_intent: 'B' },
    { reply_quote: { ...quote(), revision: 'later' } },
    { reply_to: 'other' },
    { editing: true },
  ])
    assert.equal(
      canConsumeAcceptedMediaReply({ ...accepted, editing: false, ...change }, accepted),
      false,
    );
  assert.equal(
    canConsumeAcceptedMediaReply({ reply_to: null, editing: false }, { reply_to: null }),
    false,
  );
  const value = {
    accepted_intent_json: JSON.stringify({
      input_revision: 'r1',
      messages: ['sent'],
      reply: accepted,
    }),
  };
  const receipt = mediaAcceptedIntent(value)!;
  assert.deepEqual(receipt.reply, accepted);
  assert.ok(Object.isFrozen(receipt.reply?.reply_quote?.spans));
  for (const bad of [
    { input_revision: '0', messages: ['sent'] },
    { input_revision: 'r1', messages: [] },
    { input_revision: 'r1', messages: ['sent', 'sent'] },
    { input_revision: 'r1', messages: ['sent'], extra: true },
  ])
    assert.throws(() => mediaAcceptedIntent({ accepted_intent_json: JSON.stringify(bad) }));
});

test('hidden accepted output needs an exact Core receipt; missing or mismatched acceptance retains the media', async () => {
  for (const mode of ['hidden', 'missing', 'wrong_revision', 'wrong_message'] as const) {
    const f = fixture();
    await f.channel.load();
    await f.channel.save(content(), [ingest()]);
    const revision = f.current().revision;
    f.port.send = async () => ({
      entries: mode === 'wrong_message' ? [{ id: 'unrelated' } as TimelineEntry] : [],
      draft: {
        ...empty('r2'),
        ...(mode === 'missing'
          ? {}
          : {
              accepted_intent_json: JSON.stringify({
                input_revision: mode === 'wrong_revision' ? 'other' : revision,
                messages: ['sent'],
              }),
            }),
      },
    });
    if (mode === 'hidden') {
      const accepted = await f.channel.send();
      assert.equal(accepted.entries.length, 0);
      assert.equal(f.channel.getSnapshot().content, null);
    } else {
      await assert.rejects(f.channel.send());
      assert.equal(f.channel.getSnapshot().content?.kind, 'attachments');
      assert.equal(f.channel.getSnapshot().frozen, true);
    }
  }
});

test('compiled normal composer renews reply intent on reselection, survives cold restore, and consumes only its own media receipt', async () => {
  const f = await messageDraftReactFixture();
  const source = { id: 'source', text_revision: 'source_r1', deleted: false } as TimelineEntry;
  try {
    await f.render();
    await f.act(() =>
      f.current().updateDraft({ type: 'reply', entry: source, selection: quote() }),
    );
    const first = {
      reply_to: 'source',
      reply_quote: quote(),
      reply_intent: f.current().replyIntent,
    };
    assert.ok(first.reply_intent);
    await f.act(() => f.current().flush());
    await f.reopen(undefined, true);
    assert.equal(f.current().replyIntent, first.reply_intent);
    await f.act(() =>
      f.current().updateDraft({ type: 'reply', entry: source, selection: quote() }),
    );
    const second = f.current().replyIntent;
    assert.notEqual(second, first.reply_intent);
    await f.act(() => f.current().mediaAccepted(first));
    assert.equal(f.current().replyIntent, second);
    await f.act(() => f.current().updateDraft({ type: 'text', value: 'Later unsent text' }));
    await f.act(() => f.current().mediaAccepted({ ...first, reply_intent: second }));
    assert.equal(f.current().replyTo, null);
    assert.equal(f.current().messageDraft.text, 'Later unsent text');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('compiled picker freezes its start quote and unlink preserves the latest caption without sending', async () => {
  const f = attachmentFixture();
  f.render({ replyTo: 'source', replyQuote: quote(), replyIntent: 'A' });
  await tick();
  let done!: (items: ReturnType<typeof photo>[]) => void;
  f.select(
    () =>
      new Promise((resolve) => {
        done = resolve;
      }),
  );
  const pick = f.render().pick('photo');
  await tick();
  f.render({ replyTo: 'later', replyQuote: { ...quote(), source_id: 'later' }, replyIntent: 'B' });
  done([photo()]);
  await pick;
  f.approval()?.onClose();
  await tick();
  let ui = f.render();
  assert.equal(ui.field?.scope.replyTo, 'source');
  assert.equal(ui.field?.scope.replyIntent, 'A');
  ui.field!.onChange('Latest private caption');
  const removed = ui.field!.onCancelReply!();
  assert.equal(removed?.reply_intent, 'A');
  await f.channel.flush();
  ui = f.render();
  assert.equal(ui.field?.scope.replyTo, null);
  assert.equal(ui.field?.value, 'Latest private caption');
  assert.equal(JSON.parse(f.durable().content_json).reply_quote, undefined);
  assert.deepEqual(f.sent, []);
  f.unmount();
});
