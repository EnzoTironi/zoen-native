import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { TimelineEntry } from './types';
import {
  captureMessageDraft,
  initialMessageDraft,
  messageDraftReducer as reduce,
} from './message-draft.ts';

const message = (id: string) => ({ id }) as TimelineEntry;
test('late edit completion preserves newer typing, including clear and retype of identical text', () => {
  let draft = reduce(initialMessageDraft(), { type: 'edit', entry: message('a'), text: 'First' });
  const sent = captureMessageDraft(draft);
  draft = reduce(draft, { type: 'text', value: 'New text' });
  assert.equal(reduce(draft, { type: 'accepted', attempt: sent }).editText, 'New text');
  draft = reduce(draft, { type: 'text', value: 'First' });
  assert.equal(reduce(draft, { type: 'accepted', attempt: sent }).editing?.id, 'a');
});
test('save of A never dismisses a newer edit B or a newly chosen reply', () => {
  const first = reduce(initialMessageDraft(), { type: 'edit', entry: message('a'), text: 'First' });
  const sent = captureMessageDraft(first);
  const second = reduce(first, { type: 'edit', entry: message('b'), text: 'Second' });
  assert.deepEqual(reduce(second, { type: 'accepted', attempt: sent }), second);
  const reply = reduce(first, { type: 'reply', entry: message('c') });
  assert.deepEqual(reduce(reply, { type: 'accepted', attempt: sent }), reply);
});
test('send keeps subsequent typing and a newly selected quote, even for the same message', () => {
  let draft = reduce(initialMessageDraft(), { type: 'text', value: 'Hello' });
  draft = reduce(draft, { type: 'reply', entry: message('quote') });
  const sent = captureMessageDraft(draft);
  draft = reduce(draft, { type: 'text', value: '' });
  draft = reduce(draft, { type: 'text', value: 'Hello' });
  draft = reduce(draft, { type: 'reply', entry: message('quote') });
  const result = reduce(draft, { type: 'accepted', attempt: sent });
  assert.equal(result.text, 'Hello');
  assert.equal(result.reply?.id, 'quote');
});
test('accepted exact edit returns to the preserved normal message draft; failure consumes nothing', () => {
  let draft = reduce(initialMessageDraft(), { type: 'text', value: 'Unsent normal draft' });
  draft = reduce(draft, { type: 'edit', entry: message('a'), text: 'Edit only' });
  assert.equal(draft.text, 'Unsent normal draft');
  const result = reduce(draft, { type: 'accepted', attempt: captureMessageDraft(draft) });
  assert.equal(result.text, 'Unsent normal draft');
  assert.equal(result.editing, null);
  assert.equal(result.editText, '');
});

test('media completion keeps newer quotes, edits and unrelated typed text', () => {
  let draft = reduce(initialMessageDraft(), { type: 'text', value: 'Unsent caption' });
  draft = reduce(draft, { type: 'reply', entry: message('a') });
  const contextRevision = draft.contextRevision;
  const next = reduce(draft, { type: 'reply', entry: message('b') });
  assert.deepEqual(reduce(next, { type: 'media-accepted', contextRevision }), next);
  const edit = reduce(draft, { type: 'edit', entry: message('c'), text: 'New edit' });
  assert.deepEqual(reduce(edit, { type: 'media-accepted', contextRevision }), edit);
  const accepted = reduce(draft, { type: 'media-accepted', contextRevision });
  assert.equal(accepted.reply, null);
  assert.equal(accepted.text, 'Unsent caption');
});
