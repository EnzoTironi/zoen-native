import assert from 'node:assert/strict';
import { test } from 'node:test';
import { assistantRouteFixture, deferred } from './assistant-route-react.fixtures.ts';
for (const interruption of ['blur', 'background'] as const) {
  test(`temporary answer closes on ${interruption} and resumes without replay`, async () => {
    const f = await assistantRouteFixture();
    try {
      f.launch();
      await f.mount();
      assert.deepEqual(f.opened, [f.space]);
      assert.deepEqual(f.asked, ['lease-1:Missing search question']);
      assert.ok(f.text().includes('Local answer'));
      await f.root.act(() => f.composer()!.onChangeText('Unsent follow-up'));
      if (interruption === 'blur') await f.focus(false);
      else await f.foreground('background');
      assert.deepEqual(f.closed, ['lease-1']);
      assert.equal(f.text().includes('Local answer'), false);
      if (interruption === 'blur') await f.focus(true);
      else await f.foreground('active');
      assert.equal(f.opened.length, 2);
      assert.equal(f.asked.length, 1);
      assert.equal(f.composer()?.value, 'Unsent follow-up');
      assert.equal(f.text().includes('Missing search question'), false);
      await f.root.act(() => f.composer()!.onSend());
      assert.deepEqual(f.asked, ['lease-1:Missing search question', 'lease-2:Unsent follow-up']);
      assert.equal(f.composer()?.value, '');
      assert.ok(f.calls.every((value: any) => value.op === 'snapshot'));
    } finally {
      await f.cleanup();
    }
  });
}
for (const retirement of ['owner', 'membership'] as const) {
  test(`temporary transcript and draft retire on ${retirement}`, async () => {
    const f = await assistantRouteFixture();
    try {
      f.launch();
      await f.mount();
      await f.root.act(() => f.composer()!.onChangeText('Private draft'));
      if (retirement === 'owner') await f.changeOwner();
      else await f.removeMembership();
      assert.deepEqual(f.closed, ['lease-1']);
      assert.equal(f.text().includes('Local answer'), false);
      assert.equal(f.composer(), undefined);
      await f.restoreOriginal();
      await f.focus(false);
      await f.focus(true);
      assert.equal(f.opened.length, 1);
      assert.equal(f.asked.length, 1);
    } finally {
      await f.cleanup();
    }
  });
}
test('owner change between launch and first mount cannot borrow query/context', async () => {
  const f = await assistantRouteFixture();
  try {
    f.launch();
    await f.changeOwner();
    await f.mount();
    assert.deepEqual(f.opened, []);
    assert.deepEqual(f.asked, []);
    assert.equal(f.composer(), undefined);
    assert.equal(JSON.stringify(f.routes).includes('Missing search question'), false);
  } finally {
    await f.cleanup();
  }
});
test('pending answer is discarded on blur; unanswered question remains editable on return', async () => {
  const f = await assistantRouteFixture(),
    answer = deferred<{ text: string }>();
  try {
    f.launch();
    f.holdAsk(answer.promise);
    await f.mount();
    await f.focus(false);
    assert.deepEqual(f.closed, ['lease-1']);
    await f.root.act(() => answer.resolve({ text: 'Retired answer' }));
    await f.focus(true);
    assert.equal(f.text().includes('Retired answer'), false);
    assert.equal(f.asked.length, 1);
    assert.equal(f.composer()?.value, 'Missing search question');
  } finally {
    answer.resolve({ text: 'cleanup' });
    await f.cleanup();
  }
});
test('late open is closed and drained before next focus may open', async () => {
  const f = await assistantRouteFixture(),
    opening = deferred<any>(),
    closing = deferred<void>();
  try {
    f.launch();
    f.holdOpen(opening.promise);
    f.holdClose(closing.promise);
    await f.mount();
    await f.focus(false);
    await f.focus(true);
    assert.equal(f.opened.length, 1);
    await f.root.act(() =>
      opening.resolve({ available: true, id: 'late-lease', contextMessages: 1, limited: true }),
    );
    assert.deepEqual(f.closed, ['late-lease']);
    assert.deepEqual(f.asked, []);
    f.holdOpen(null);
    f.holdClose(null);
    await f.root.act(() => closing.resolve());
    assert.equal(f.opened.length, 2);
    assert.deepEqual(f.asked, []);
    assert.equal(f.composer()?.value, 'Missing search question');
  } finally {
    opening.resolve({ available: false, reason: 'unsupported' });
    closing.resolve();
    await f.cleanup();
  }
});

test('a discarded/cold launch ticket cannot recreate the private query or start inference', async () => {
  const f = await assistantRouteFixture();
  try {
    f.launch();
    f.release();
    await f.mount();
    assert.deepEqual(f.opened, []);
    assert.deepEqual(f.asked, []);
    assert.equal(f.composer(), undefined);
  } finally {
    await f.cleanup();
  }
});
test('unacknowledged cleanup blocks a new lease until explicit retry closes the same lease', async () => {
  const f = await assistantRouteFixture(),
    closing = deferred<void>();
  try {
    f.launch();
    await f.mount();
    await f.root.act(() => f.composer()!.onChangeText('Preserved private follow-up'));
    f.holdClose(closing.promise);
    await f.focus(false);
    await f.root.act(() => closing.reject(Error('cleanup unknown')));
    await f.focus(true);
    assert.equal(f.opened.length, 1);
    assert.ok(f.buttons.has('agents.tryAgain'));
    f.holdClose(null);
    await f.root.act(() => f.buttons.get('agents.tryAgain')!.onPress());
    assert.deepEqual(f.closed, ['lease-1', 'lease-1']);
    assert.equal(f.opened.length, 2);
    assert.equal(f.asked.length, 1);
    assert.equal(f.composer()?.value, 'Preserved private follow-up');
  } finally {
    f.holdClose(null);
    closing.resolve();
    await f.cleanup();
  }
});
test('unsupported on-device model never fabricates an answer or falls back to a provider', async () => {
  const f = await assistantRouteFixture();
  try {
    f.launch();
    f.holdOpen(Promise.resolve({ available: false, reason: 'not_eligible' }));
    await f.mount();
    assert.equal(f.opened.length, 1);
    assert.deepEqual(f.asked, []);
    assert.equal(f.composer(), undefined);
    assert.ok(f.calls.every((value: any) => value.op === 'snapshot'));
  } finally {
    await f.cleanup();
  }
});

test('same-owner account reopen retires the old route and cannot reuse its launch ticket', async () => {
  const f = await assistantRouteFixture();
  try {
    f.launch();
    await f.mount();
    await f.reopen();
    assert.deepEqual(f.closed, ['lease-1']);
    assert.equal(f.composer(), undefined);
    await f.focus(false);
    await f.focus(true);
    assert.equal(f.opened.length, 1);
    assert.equal(f.asked.length, 1);
  } finally {
    await f.cleanup();
  }
});
