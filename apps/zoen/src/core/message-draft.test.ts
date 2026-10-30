import assert from 'node:assert/strict';
import { test } from 'node:test';
import { canConsumeMediaReply } from './message-draft.ts';

test('captured media acceptance checks live quote and edit fences independently', () => {
  const sent = { replyRevision: 3, editContextRevision: 2 };
  assert.equal(canConsumeMediaReply({ ...sent, editing: false }, sent), true);
  assert.equal(canConsumeMediaReply({ ...sent, replyRevision: 4, editing: false }, sent), false);
  assert.equal(
    canConsumeMediaReply({ ...sent, editContextRevision: 3, editing: false }, sent),
    false,
  );
  assert.equal(canConsumeMediaReply({ ...sent, editing: true }, sent), false);
  // The summed numbers match, but this is a different quote after navigation.
  assert.equal(
    canConsumeMediaReply({ replyRevision: 5, editContextRevision: 0, editing: false }, sent),
    false,
  );
});
