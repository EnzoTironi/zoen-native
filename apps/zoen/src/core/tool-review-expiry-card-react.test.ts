import assert from 'node:assert/strict';
import { test } from 'node:test';
import { reviewPolicyReactFixture } from './review-policy-react.fixtures.ts';

for (const resume of [false, true])
  test(`mounted pending drawing card observes expiry with its recovery child: ${resume ? 'foreground' : 'deadline'}`, async (t) => {
    t.mock.timers.enable({ apis: ['Date', 'setTimeout'], now: 1_800_000_000_000 });
    const f = await reviewPolicyReactFixture('card');
    try {
      f.request.status = 'Pending';
      const tool = f.request.tool_approval!;
      tool.canonical_tool = 'read_scene';
      tool.expires_ms = String(Date.now() + 1_000);
      tool.review!.state = 'Unavailable';
      await f.render();
      assert.deepEqual(f.errors, []);
      const before = f.buttons.get('agents.allowOnce');
      assert.ok(before);
      assert.equal(before.disabled, false);
      assert.equal(f.buttons.has('reviewRenewal.renew'), false);
      const reads = f.calls.filter((command) => command.op === 'requests').length;
      if (resume) await f.act(() => f.foreground('background'));
      await f.act(() => t.mock.timers.tick(1_001));
      if (resume) {
        assert.equal(f.calls.filter((command) => command.op === 'requests').length, reads);
        await f.act(() => f.foreground('active'));
      }
      assert.equal(f.buttons.get('agents.allowOnce')?.disabled, true);
      assert.equal(f.buttons.get('reviewRenewal.renew')?.disabled, false);
      assert.equal(f.calls.filter((command) => command.op === 'requests').length, reads + 1);
      assert.equal(f.request.status, 'Pending', 'child metadata does not replace the parent DTO');
      assert.equal(tool.review!.state, 'Unavailable');
      assert.doesNotMatch(f.visibleText(), /agents\.theContentHasChanged/);
      await f.act(() => before.onPress());
      assert.equal(
        f.calls.filter((command) => command.op === 'decide_tool_request').length,
        0,
        'retained approval callback independently rejects the elapsed deadline',
      );
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
