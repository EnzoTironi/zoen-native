import test from 'node:test';
import assert from 'node:assert/strict';
import { nativeFixture, nativeFixtureConversations } from './native-fixture-store.ts';
import { PARITY_FIXTURE } from './parity-config.ts';
test('native fixture adapter preserves actual core content, IDs, cards and timestamps without normalization', () => {
  const result = nativeFixtureConversations();
  assert.equal(result.length, nativeFixture.spaces.length);
  for (const [key, expected] of Object.entries(PARITY_FIXTURE)) assert.equal(nativeFixture[key as keyof typeof nativeFixture], expected);
  for (const [index, conversation] of result.entries()) {
    const source = nativeFixture.spaces[index]!;
    assert.equal(conversation.id, source.id);
    assert.equal(conversation.messages.length, source.entries.length);
    for (const [position, entry] of source.entries.entries()) {
      const message = conversation.messages[position]!;
      assert.equal(message.id, entry.id); assert.equal(message.author, entry.author.id);
      assert.equal(message.at_ms, entry.atMs); assert.equal(message.text, entry.text);
      assert.deepEqual(message.card ?? null, entry.card);
    }
  }
});
