import assert from 'node:assert/strict';
import { test } from 'node:test';
import { messageMenuActions } from '../components/message-actions-model.ts';
import { plainMessage } from './message-caption.fixtures.ts';
import type { TimelineEntry } from './types';
const message = (patch: Partial<TimelineEntry> = {}) => ({ ...plainMessage(), ...patch });
import type { MessageActionsProps } from '../components/message-actions.types';

function props(entry: MessageActionsProps['entry']): MessageActionsProps {
  const run = () => {};
  return {
    entry,
    mode: 'actions',
    onClose: run,
    onReact: run,
    onReply: run,
    onThread: run,
    onInfo: run,
    onForward: run,
    onSelect: run,
    onSelectText: run,
    onCopy: run,
    onCopyLink: run,
    onShare: run,
    onEdit: run,
    onRecover: run,
    onDelete: run,
  };
}

test('unavailable leaves expose only local selection/deletion, never content actions', () => {
  const entry = message({ deleted: true, text_revision: undefined, thread_replies: 0 });
  const rows = messageMenuActions(props(entry));
  assert.deepEqual(
    rows.map((row) => row.id),
    ['selectMessages', 'delete'],
  );
  assert.equal(rows.find((row) => row.id === 'delete')?.destructive, true);
  assert.deepEqual(
    messageMenuActions(props({ ...entry, thread_replies: 3 })).map((row) => row.id),
    ['thread'],
  );
});

test('received messages keep private deletion without acquiring author-only edits or recovery', () => {
  const entry = message({
    author: { ...plainMessage().author, id: 'peer', is_me: false },
    delivery: 'Failed',
  });
  const ids = messageMenuActions(props(entry)).map((row) => row.id);
  assert.ok(ids.includes('delete') && ids.includes('selectMessages') && ids.includes('forward'));
  assert.ok(!ids.includes('edit') && !ids.includes('recover') && !ids.includes('select'));
  assert.deepEqual(
    messageMenuActions({ ...props(entry), onDelete: undefined }).some((row) => row.id === 'delete'),
    false,
  );
});

test('service rows and an absent source never gain message actions from supplied callbacks', () => {
  const entry = message({ kind: { System: { text: 'Update' } } });
  assert.deepEqual(messageMenuActions(props(entry)), []);
  assert.deepEqual(messageMenuActions(props(null)), []);
  assert.deepEqual(messageMenuActions(null), []);
});
