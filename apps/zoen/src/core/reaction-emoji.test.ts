import assert from 'node:assert/strict';
import test from 'node:test';
import { MAX_REACTION_EMOJI_BYTES, normalizeReactionEmoji } from './reaction-emoji.ts';

test('reaction input accepts single Unicode emoji sequences rather than single code points', () => {
  for (const emoji of ['👍', '❤️', '🇧🇷', '👍🏽', '👩🏽‍💻', '👨‍👩‍👧‍👦', '1️⃣', '🏳️‍🌈', '🫩']) {
    assert.ok(Buffer.byteLength(emoji) <= MAX_REACTION_EMOJI_BYTES);
    assert.equal(normalizeReactionEmoji(emoji), emoji);
  }
  assert.equal(normalizeReactionEmoji(' \n🇧🇷\t '), '🇧🇷');
});

test('reaction input rejects prose, separate emoji and malformed sequences', () => {
  for (const input of [
    '',
    ' \n ',
    'hello',
    '1',
    '😀😀',
    '🇧🇷 👍',
    'a👍',
    '👍a',
    '\u200d',
    '\ufe0f',
    '\ud83d',
    '👩\u200d',
  ]) {
    assert.equal(normalizeReactionEmoji(input), null, JSON.stringify(input));
  }
});

test('reaction input preserves canonical identity and the core UTF-8 byte bound', () => {
  const tooLarge = '👩🏿‍❤️‍💋‍👩🏾';
  assert.ok(Buffer.byteLength(tooLarge) > MAX_REACTION_EMOJI_BYTES);
  assert.equal(normalizeReactionEmoji(tooLarge), null);
  assert.equal(normalizeReactionEmoji('❤'), '❤');
  assert.equal(normalizeReactionEmoji('❤️'), '❤️');
  // A global regex must not retain a previous match's cursor across repeated choices.
  assert.equal(normalizeReactionEmoji('👍🏽'), '👍🏽');
  assert.equal(normalizeReactionEmoji('👍🏽'), '👍🏽');
});
