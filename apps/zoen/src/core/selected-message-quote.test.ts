import assert from 'node:assert/strict';
import { test } from 'node:test';
import { selectedMessageQuote } from './selected-message-quote.ts';
import { NormalMessageDrafts, type NormalDraftSnapshot } from './normal-message-drafts.ts';
import { main } from './normal-message-drafts.fixtures.ts';
import type { ReplySelection } from './generated/ReplySelection';
import type { TimelineEntry } from './types';
const source = '😀 same **same** [ação](https://example.invalid)';
const entry = {
  id: 'message',
  text_revision: 'edit-event',
  deleted: false,
  kind: { Message: { text: source, card: null } },
} as unknown as TimelineEntry;
const ranges = (start: number, end: number) =>
  JSON.stringify([{ start_utf16: start, end_utf16: end }]);
test('rendered selection is bound to the exact repeated Unicode source spans', () => {
  assert.deepEqual(selectedMessageQuote(entry, 'same', ranges(10, 14))?.spans, [
    { start_utf16: 10, end_utf16: 14 },
  ]);
  const link = source.indexOf('ação');
  assert.equal(
    selectedMessageQuote(
      entry,
      'same ação',
      JSON.stringify([
        { start_utf16: 10, end_utf16: 14 },
        { start_utf16: 16, end_utf16: 17 },
        { start_utf16: link, end_utf16: link + 4 },
      ]),
    )?.text,
    'same ação',
  );
});
test('missing provenance, surrogate splits, overlaps and false excerpts never stage', () => {
  for (const [text, spans] of [
    ['same', undefined],
    ['same', '[]'],
    ['same', ranges(1, 2)],
    ['😀', ranges(0, 1)],
    ['fake', ranges(10, 14)],
    [
      'samesame',
      JSON.stringify([
        { start_utf16: 10, end_utf16: 14 },
        { start_utf16: 10, end_utf16: 14 },
      ]),
    ],
  ] as const)
    assert.equal(selectedMessageQuote(entry, text, spans), null);
  assert.equal(selectedMessageQuote({ ...entry, deleted: true }, 'same', ranges(10, 14)), null);
});
test('quote survives existing draft CAS, cold load and unknown ACK without automatic send', async () => {
  const selection = selectedMessageQuote(entry, 'same', ranges(10, 14))!;
  let stored: NormalDraftSnapshot = { revision: '0', text: '', reply_to: null };
  let revision = 0,
    sends = 0;
  let accepted: {
    message_id: string;
    text_revision: string | null;
    entry: TimelineEntry;
    draft: NormalDraftSnapshot;
  } | null = null;
  const port = {
    load: async () => structuredClone(stored),
    save: async (
      _: unknown,
      expected: string,
      text: string,
      reply: string | null,
      quote?: ReplySelection | null,
    ) => {
      assert.equal(stored.revision, expected);
      stored = {
        revision: String(++revision),
        text,
        reply_to: reply,
        reply_quote: quote,
      };
      return structuredClone(stored);
    },
    send: async (_: unknown, expected: string) => {
      if (accepted) return accepted;
      assert.equal(stored.revision, expected);
      assert.deepEqual(stored.reply_quote, selection);
      sends++;
      stored = {
        revision: String(++revision),
        text: '',
        reply_to: null,
        reply_quote: null,
      };
      accepted = {
        message_id: entry.id,
        text_revision: entry.text_revision ?? null,
        entry,
        draft: structuredClone(stored),
      };
      throw new Error('unknown ACK');
    },
  };
  let drafts = new NormalMessageDrafts(port);
  await drafts.activate(main);
  drafts.setText(main, 'my draft');
  drafts.setReply(main, entry.id, selection);
  await drafts.flush(main);
  assert.equal(sends, 0);
  await drafts.retire({ flush: true });
  drafts = new NormalMessageDrafts(port);
  await drafts.activate(main);
  assert.deepEqual(drafts.getSnapshot(main).replyQuote, selection);
  await assert.rejects(drafts.send(main), /unknown ACK/);
  assert.deepEqual(drafts.getSnapshot(main).replyQuote, selection);
  await drafts.send(main);
  assert.equal(sends, 1);
  assert.equal(drafts.getSnapshot(main).replyQuote, null);
});
