import assert from 'node:assert/strict';
import { test } from 'node:test';
import { reviewPolicyReactFixture } from './review-policy-react.fixtures.ts';
import { deferred } from './store.fixtures.ts';
import type { DecideOutcome } from './generated/DecideOutcome';

for (const interruption of [
  'blur',
  'background',
  'reopen',
  'identity',
  'device',
  'relay',
  'canonical',
] as const) {
  test(`standing decision confirmation cannot submit after ${interruption}`, async () => {
    const consent = deferred<boolean>();
    const f = await reviewPolicyReactFixture('card', { confirm: () => consent.promise });
    try {
      await f.render();
      const button = f.buttons.get('agents.alwaysAllowThisType');
      assert.ok(button);
      await f.act(() => button.onPress());
      if (interruption === 'blur') await f.act(() => f.focus(false));
      if (interruption === 'background') await f.act(() => f.foreground('background'));
      if (interruption === 'reopen') await f.reopen();
      if (interruption === 'identity' || interruption === 'device' || interruption === 'relay')
        await f.reopen(interruption);
      if (interruption === 'canonical') {
        f.request.status = 'Stale';
        await f.act(() => f.store.refresh());
      }
      await f.act(() => consent.resolve(true));
      assert.equal(f.calls.filter((call) => call.op.startsWith('decide_')).length, 0);
    } finally {
      consent.resolve(false);
      await f.close();
    }
  });
}

test('one request cannot open concurrent standing consent dialogs', async () => {
  const consent = deferred<boolean>();
  let confirmations = 0;
  const f = await reviewPolicyReactFixture('card', {
    confirm: () => {
      confirmations++;
      return consent.promise;
    },
  });
  try {
    await f.render();
    const button = f.buttons.get('agents.alwaysAllowThisType');
    assert.ok(button);
    await f.act(() => {
      button.onPress();
      button.onPress();
    });
    assert.equal(confirmations, 1);
  } finally {
    consent.resolve(false);
    await f.close();
  }
});

for (const harmless of ['refresh', 'preflight'] as const)
  test(`a harmless ${harmless} does not discard current standing consent`, async () => {
    const consent = deferred<boolean>();
    let resolved = 0;
    const f = await reviewPolicyReactFixture('card', {
      confirm: () => consent.promise,
      resolved: () => {
        resolved++;
      },
    });
    try {
      await f.render();
      await f.act(() => f.buttons.get('agents.alwaysAllowThisType')!.onPress());
      if (harmless === 'refresh') await f.act(() => f.store.refresh());
      else await f.act(() => f.store.assertExternalNavigationAllowed());
      await f.act(() => consent.resolve(true));
      assert.equal(f.calls.filter((call) => call.op === 'decide_request').length, 1);
      assert.equal(resolved, 1);
      assert.ok(f.visibleText().includes('Accepted decision'));
    } finally {
      consent.resolve(false);
      await f.close();
    }
  });

test('a retained approval button cannot borrow a later activation; a fresh press still works', async () => {
  let resolved = 0;
  const f = await reviewPolicyReactFixture('card', {
    confirm: async () => true,
    resolved: () => {
      resolved++;
    },
  });
  try {
    await f.render();
    const retained = f.buttons.get('agents.allowOnce');
    assert.ok(retained);
    await f.act(() => f.focus(false));
    await f.act(() => f.focus(true));
    await f.act(() => retained.onPress());
    assert.equal(f.calls.filter((call) => call.op === 'decide_request').length, 0);
    await f.act(() => f.buttons.get('agents.allowOnce')!.onPress());
    assert.equal(f.calls.filter((call) => call.op === 'decide_request').length, 1);
    assert.equal(resolved, 1);
    assert.ok(f.visibleText().includes('Accepted decision'));
  } finally {
    await f.close();
  }
});

test('current standing consent survives its own real Store canonical notification and resolves once', async () => {
  let resolved = 0;
  let f: Awaited<ReturnType<typeof reviewPolicyReactFixture>>;
  f = await reviewPolicyReactFixture('card', {
    confirm: async () => true,
    submit: async () => {
      f.request.status = 'Approved';
      return {
        request: f.request,
        message: 'Canonical decision accepted',
        standing_grant_id: 'grant',
        also_resolved: 2,
      };
    },
    resolved: () => {
      resolved++;
    },
  });
  try {
    await f.render();
    const retained = f.buttons.get('agents.alwaysAllowThisType');
    assert.ok(retained);
    await f.act(() => retained.onPress());
    assert.equal(f.calls.filter((call) => call.op === 'decide_request').length, 1);
    assert.equal(resolved, 1);
    assert.ok(f.visibleText().includes('agents.alsoResolved'));
    await f.act(() => retained.onPress());
    assert.equal(f.calls.filter((call) => call.op === 'decide_request').length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('declining standing consent performs no write and a later error after retirement stays quiet', async () => {
  const response = deferred<DecideOutcome>();
  let resolved = 0;
  const f = await reviewPolicyReactFixture('card', {
    confirm: async () => false,
    submit: () => response.promise,
    resolved: () => {
      resolved++;
    },
  });
  try {
    await f.render();
    await f.act(() => f.buttons.get('agents.alwaysDenyThisType')!.onPress());
    assert.equal(f.calls.filter((call) => call.op.startsWith('decide_')).length, 0);
    await f.act(() => f.buttons.get('agents.doNotAllow')!.onPress());
    assert.equal(f.calls.filter((call) => call.op === 'decide_request').length, 1);
    await f.act(() => f.foreground('background'));
    await f.act(() => response.reject(Error('Retired decision error')));
    assert.equal(resolved, 0);
    assert.equal(f.visibleText().includes('Retired decision error'), false);
    assert.equal(f.visibleText().includes('agents.thisDecisionCouldNotBeSaved'), false);
    assert.deepEqual(f.errors, []);
  } finally {
    response.resolve({
      request: f.request,
      message: '',
      standing_grant_id: null,
      also_resolved: 0,
    });
    await f.close();
  }
});

test('admitted decision result cannot publish into a later focus activation', async () => {
  const response = deferred<DecideOutcome>();
  let resolved = 0;
  const f = await reviewPolicyReactFixture('card', {
    confirm: async () => true,
    submit: () => response.promise,
    resolved: () => {
      resolved++;
    },
  });
  try {
    await f.render();
    const button = f.buttons.get('agents.allowOnce');
    assert.ok(button);
    await f.act(() => button.onPress());
    assert.equal(f.calls.filter((call) => call.op === 'decide_request').length, 1);
    await f.act(() => f.focus(false));
    await f.act(() => f.focus(true));
    await f.act(() =>
      response.resolve({
        request: { ...f.request, status: 'Approved' },
        message: 'Retired decision result',
        standing_grant_id: null,
        also_resolved: 0,
      }),
    );
    assert.equal(resolved, 0);
    assert.equal(f.visibleText().includes('Retired decision result'), false);
    assert.equal(f.calls.filter((call) => call.op === 'decide_request').length, 1);
  } finally {
    response.resolve({
      request: f.request,
      message: '',
      standing_grant_id: null,
      also_resolved: 0,
    });
    await f.close();
  }
});
