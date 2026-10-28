import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  selectionMessageIds,
  MAX_SELECTED_MESSAGES,
  selectedMessages,
  selectedMessageText,
  toggleMessageSelection,
} from './message-selection.ts';
import type { TimelineEntry } from './types';

function entry(id: string, text = id, patch: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id,
    kind: { Message: { text, card: null } },
    deleted: false,
    voice: null,
    ...patch,
  } as TimelineEntry;
}

test('selection follows timeline order, toggles independently and ends at the last deselection', () => {
  const entries = [entry('first'), entry('second')];
  let value = toggleMessageSelection(null, 'account:device:epoch:chat', entries, 'second');
  value = toggleMessageSelection(value, 'account:device:epoch:chat', entries, 'first');
  assert.deepEqual(
    selectedMessages(value, 'account:device:epoch:chat', entries).map((value) => value.id),
    ['first', 'second'],
  );
  value = toggleMessageSelection(value, 'account:device:epoch:chat', entries, 'second');
  assert.deepEqual(value?.ids, ['first']);
  assert.equal(toggleMessageSelection(value, 'account:device:epoch:chat', entries, 'first'), null);
});

test('account, device, epoch, conversation and access changes retire every selected identity', () => {
  const entries = [entry('one')];
  const selected = toggleMessageSelection(null, 'a:phone:1:chat', entries, 'one');
  for (const scope of [
    'b:phone:1:chat',
    'a:tablet:1:chat',
    'a:phone:2:chat',
    'a:phone:1:other',
    null,
  ])
    assert.deepEqual(selectedMessages(selected, scope, entries), []);
});

test('current signed entries decide eligibility and copied content, including edits and removal', () => {
  const entries = [entry('a'), entry('b')];
  const selected = toggleMessageSelection(
    toggleMessageSelection(null, 'scope', entries, 'a'),
    'scope',
    entries,
    'b',
  );
  const current = [entry('a', 'edited'), entry('b', 'removed', { deleted: true })];
  assert.equal(selectedMessageText(selectedMessages(selected, 'scope', current)), 'edited');
  assert.deepEqual(selectedMessages(selected, 'scope', []), []);
  assert.equal(toggleMessageSelection(null, 'scope', current, 'b'), null);
});

test('copy retains exact text and skips voice, deleted and blank messages', () => {
  assert.equal(
    selectedMessageText([
      entry('a', ' exact text '),
      entry('voice', 'transcript', { voice: {} as TimelineEntry['voice'] }),
      entry('blank', ' '),
      entry('deleted', 'gone', { deleted: true }),
      entry('b', 'second'),
    ]),
    ' exact text \n\nsecond',
  );
});

test('forward navigation retains ordered unique identities and rejects malformed queues', () => {
  assert.deepEqual(selectionMessageIds('["a","b","a","c"]'), ['a', 'b', 'c']);
  assert.deepEqual(selectionMessageIds('["a"]'), ['a']);
  for (const malformed of ['[1]', '{}', '[" "]', 'not json'])
    assert.deepEqual(selectionMessageIds(malformed), []);
});

test('selection and route share the 100-message bound without preventing deselection or reuse', () => {
  const entries = Array.from({ length: MAX_SELECTED_MESSAGES + 1 }, (_, index) =>
    entry(`id-${index}`),
  );
  let selected = toggleMessageSelection(null, 'scope', entries, entries[0]!.id);
  for (const value of entries.slice(1, MAX_SELECTED_MESSAGES))
    selected = toggleMessageSelection(selected, 'scope', entries, value.id);
  assert.equal(selectedMessages(selected, 'scope', entries).length, MAX_SELECTED_MESSAGES);
  assert.equal(toggleMessageSelection(selected, 'scope', entries, entries.at(-1)!.id), selected);
  selected = toggleMessageSelection(selected, 'scope', entries, entries[0]!.id);
  selected = toggleMessageSelection(selected, 'scope', entries, entries.at(-1)!.id);
  assert.equal(selectedMessages(selected, 'scope', entries).length, MAX_SELECTED_MESSAGES);
  assert.equal(selected!.ids.includes(entries.at(-1)!.id), true);
  assert.equal(selected!.ids.includes(entries[0]!.id), false);
  assert.equal(
    selectionMessageIds(
      JSON.stringify(entries.slice(0, MAX_SELECTED_MESSAGES).map((value) => value.id)),
    ).length,
    MAX_SELECTED_MESSAGES,
  );
  assert.deepEqual(selectionMessageIds(JSON.stringify(entries.map((value) => value.id))), []);
  assert.deepEqual(selectionMessageIds('[]'), []);
});
