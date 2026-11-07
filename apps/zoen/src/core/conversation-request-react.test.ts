import assert from 'node:assert/strict';
import { test } from 'node:test';
import { conversationRequestFixture } from './conversation-request-react.fixtures.ts';

for (const [label, decision] of [
  ['agents.allowOnce', 'ApproveOnce'],
  ['agents.doNotAllow', 'Deny'],
  ['agents.cancelRequest', 'Cancel'],
] as const) {
  test(`the compiled conversation review button sends only the exact native ${decision} command`, async () => {
    const f = await conversationRequestFixture();
    try {
      await f.press(label);
      assert.equal(f.commands.length, 1);
      assert.deepEqual(structuredClone(f.commands[0]), {
        op: 'decide_tool_request',
        request_id: f.request.id,
        observed_content_hash: f.request.tool_approval?.content_hash,
        decision,
      });
      assert.equal(f.resolved(), 1);
      assert.equal(
        f.snapshot.requests[0]?.status,
        decision === 'ApproveOnce' ? 'Approved' : decision === 'Deny' ? 'Denied' : 'Cancelled',
      );
      assert.equal(f.buttons.has('agents.allowOnce'), false);
    } finally {
      await f.close();
    }
  });
}

test('an old visible consent cannot act after reopening, while the fresh consent still works', async () => {
  const f = await conversationRequestFixture();
  try {
    const old = f.buttons.get('agents.allowOnce');
    assert.ok(old);
    await f.reopen();
    await f.root.act(old.onPress);
    assert.equal(f.commands.length, 0);
    await f.press('agents.allowOnce');
    assert.equal(f.commands.length, 1);
    assert.equal(f.resolved(), 1);
  } finally {
    await f.close();
  }
});
