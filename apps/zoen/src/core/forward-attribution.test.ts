import assert from 'node:assert/strict';
import { test } from 'node:test';
import { messageForwardAuthor, messageForwardLabel } from './message-presentation.ts';

const original = { forwarded: true, deleted: false, forwarded_author: 'Original 雨 😀' };
test('named forwarding uses the exact snapshot without a source or peer link', () => {
  assert.equal(messageForwardAuthor(original), 'Original 雨 😀');
  assert.equal(messageForwardLabel(original), 'Forwarded from Original 雨 😀');
  assert.equal(
    messageForwardAuthor({ ...original, forwarded_author: '雨'.repeat(64) }),
    '雨'.repeat(64),
  );
});
test('legacy/anonymous forwarding remains generic rather than borrowing the current sender', () => {
  assert.equal(messageForwardAuthor({ forwarded: true }), null);
  assert.equal(messageForwardLabel({ forwarded: true }), 'Forwarded');
  assert.equal(messageForwardLabel({ ...original, forwarded_author: null }), 'Forwarded');
});
test('ordinary and deleted messages cannot expose retained forward metadata', () => {
  for (const value of [
    { ...original, forwarded: false },
    { ...original, deleted: true },
  ]) {
    assert.equal(messageForwardAuthor(value), null);
    assert.equal(messageForwardLabel(value), null);
  }
});
test('untrusted display metadata falls back safely without altering the supplied snapshot', () => {
  for (const name of [
    '',
    ' leading',
    'trailing ',
    'two\nlines',
    'fake\u202ename',
    'a'.repeat(65),
  ]) {
    const value = { ...original, forwarded_author: name };
    assert.equal(messageForwardAuthor(value), null);
    assert.equal(messageForwardLabel(value), 'Forwarded');
    assert.equal(value.forwarded_author, name);
  }
});
