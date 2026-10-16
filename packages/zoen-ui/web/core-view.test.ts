import test from 'node:test';
import assert from 'node:assert/strict';
import { projectCurrentConversations, updatesConversations } from './core-view.ts';
test('read projection preserves opaque state and sealed frame bytes while consuming the current native view', () => {
  const update = { snapshot: '{"opaque":"  state \\n"}', frames: [[0, 255, 17, 34]], conversations: [{ id: 'old-time-view' }], pending: 7 };
  const conversations = [{ id: 'current', messages: [{ card: { snapshot: { value: '12:00' } } }] }];
  const result = JSON.parse(projectCurrentConversations(JSON.stringify(update), JSON.stringify(conversations)));
  assert.equal(result.snapshot, update.snapshot); assert.deepEqual(result.frames, update.frames); assert.equal(result.pending, update.pending);
  assert.deepEqual(result.conversations, conversations);
  assert.equal(updatesConversations('queue_failed_file'), true); assert.equal(updatesConversations('receive'), true);
  assert.equal(updatesConversations('seal_file'), false); assert.equal(updatesConversations('local_chunk'), false); assert.equal(updatesConversations('restore'), false);
  assert.throws(() => projectCurrentConversations('{}', '[]'));
});
