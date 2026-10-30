import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  NormalMessageDrafts,
  type NormalDraftSnapshot,
  type NormalDraftPort,
} from './normal-message-drafts.ts';
import { main } from './normal-message-drafts.fixtures.ts';
import type { ReplySelection } from './generated/ReplySelection';
import type { SharedReplyDraft } from './generated/SharedReplyDraft';

const selection = (): ReplySelection => ({
  source_id: 'source',
  revision: 'reviewed-r1',
  text: 'one',
  spans: [{ start_utf16: 0, end_utf16: 3 }],
});
const shared = (): SharedReplyDraft => ({
  space_id: 'other-space',
  message_id: 'source',
  revision: 'reviewed-r1',
  author_id: 'author',
  author_name: 'Author',
  text: 'one',
  selection: selection(),
});

function fixture(initial: NormalDraftSnapshot) {
  let value = initial;
  let serial = 1;
  const port: NormalDraftPort = {
    load: async () => value,
    save: async (_scope, expected, text, reply_to, reply_quote, shared_reply) => {
      assert.equal(expected, value.revision);
      value = {
        revision: String(++serial),
        text,
        reply_to,
        ...(reply_quote ? { reply_quote } : {}),
        ...(shared_reply ? { shared_reply } : {}),
      };
      return value;
    },
    send: async () => assert.fail('context inspection must not publish'),
  };
  return { drafts: new NormalMessageDrafts(port), stored: () => value };
}

for (const kind of ['selection', 'shared'] as const) {
  test(`${kind} loaded context is an owned immutable snapshot, not a mutable alias of Core IO`, async () => {
    const quote = selection(),
      cross = shared();
    const input: NormalDraftSnapshot = {
      revision: '1',
      text: 'Retained text',
      reply_to: kind === 'selection' ? 'source' : null,
      ...(kind === 'selection' ? { reply_quote: quote } : { shared_reply: cross }),
    };
    const f = fixture(input);
    await f.drafts.activate(main);
    const before = f.drafts.getSnapshot(main);
    const expected = structuredClone(kind === 'selection' ? before.replyQuote : before.sharedReply);
    quote.text = 'Changed caller input';
    quote.spans[0]!.end_utf16 = 2;
    cross.author_name = 'Changed caller author';
    cross.selection!.spans[0]!.end_utf16 = 1;
    const snapshot = kind === 'selection' ? before.replyQuote : before.sharedReply?.selection;
    assert.ok(snapshot);
    assert.ok(Object.isFrozen(snapshot));
    assert.ok(Object.isFrozen(snapshot.spans));
    assert.ok(Object.isFrozen(snapshot.spans[0]));
    assert.throws(() => {
      snapshot.spans[0]!.end_utf16 = 99;
    }, TypeError);
    assert.deepEqual(kind === 'selection' ? before.replyQuote : before.sharedReply, expected);
    f.drafts.setText(main, 'New private text');
    await f.drafts.flush(main);
    assert.deepEqual(
      kind === 'selection' ? f.stored().reply_quote : f.stored().shared_reply,
      expected,
    );
  });

  test(`${kind} setter owns and freezes its complete context without freezing the caller's input`, async () => {
    const f = fixture({ revision: '1', text: '', reply_to: null });
    await f.drafts.activate(main);
    const quote = selection(),
      cross = shared();
    assert.equal(
      kind === 'selection'
        ? f.drafts.setReply(main, 'source', quote)
        : f.drafts.setSharedReply(main, cross),
      true,
    );
    const state = f.drafts.getSnapshot(main);
    const expected = structuredClone(kind === 'selection' ? state.replyQuote : state.sharedReply);
    quote.spans.push({ start_utf16: 4, end_utf16: 5 });
    cross.selection!.spans[0]!.start_utf16 = 1;
    const selected = kind === 'selection' ? state.replyQuote : state.sharedReply?.selection;
    assert.ok(selected);
    assert.ok(Object.isFrozen(selected.spans));
    assert.throws(() => selected.spans.push({ start_utf16: 4, end_utf16: 5 }), TypeError);
    await f.drafts.flush(main);
    assert.deepEqual(
      kind === 'selection' ? f.stored().reply_quote : f.stored().shared_reply,
      expected,
    );
  });
}
