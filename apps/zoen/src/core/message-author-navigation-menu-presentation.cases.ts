import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  anchorMeasurement,
  deferred,
  fixture,
  invalidSessions,
  liftedAnchor,
  message,
  settledAnchor,
  spaceId,
} from './message-author-navigation.fixtures.ts';

test('message actions wait for native keyboard dismissal and fresh measurement before presenting', async () => {
  const f = fixture();
  const dismissed = deferred<void>();
  const measured = anchorMeasurement();
  let measurements = 0;
  f.dismiss(() => dismissed.promise);
  const pending = f.actions.open(message(), 'reactions', liftedAnchor, undefined, () => {
    ++measurements;
    return measured.measure();
  });
  assert.equal(f.dismisses(), 1);
  assert.equal(measurements, 0);
  assert.equal(f.render().selectedId, null);
  dismissed.resolve(undefined);
  await measured.entered;
  assert.equal(measurements, 1);
  assert.equal(f.render().selectedId, null);
  measured.resolve(settledAnchor);
  await pending;
  const shown = f.render();
  assert.equal(shown.selectedId, message().id);
  assert.equal(shown.panel.props.anchor, settledAnchor);
  assert.deepEqual(f.mutations, []);
  assert.deepEqual(f.alerts, []);
});

test('an explicit missing or invalid fresh anchor cancels instead of reusing the lifted position', async () => {
  for (const anchor of [
    null,
    { ...settledAnchor, width: 0 },
    { ...settledAnchor, height: 0 },
    { ...settledAnchor, y: Number.NaN },
    { ...settledAnchor, x: Number.POSITIVE_INFINITY },
  ]) {
    const f = fixture();
    await f.actions.open(message(), 'reactions', liftedAnchor, undefined, async () => anchor);
    assert.equal(f.render().selectedId, null);
    assert.deepEqual(f.alerts, []);
    await f.actions.open(message(), 'actions', liftedAnchor, undefined, async () => settledAnchor);
    assert.equal(f.render().panel.props.anchor, settledAnchor);
  }
});

test('callers without a live measurer retain their optional anchor and centered fallback', async () => {
  for (const anchor of [liftedAnchor, undefined]) {
    const f = fixture();
    await f.actions.open(message(), 'actions', anchor);
    assert.equal(f.render().selectedId, message().id);
    assert.equal(f.render().panel.props.anchor, anchor);
  }
});

test('duplicate opens share one dismissal while a different message supersedes the pending intent', async () => {
  const f = fixture();
  const other = message({ id: 'other_message' });
  const actions = f.render({ entries: [message(), other] });
  const dismissed = deferred<void>();
  const measured = anchorMeasurement();
  let obsoleteMeasurements = 0;
  f.dismiss(() => dismissed.promise);
  const old = actions.open(message(), 'reactions', undefined, undefined, async () => {
    ++obsoleteMeasurements;
    return liftedAnchor;
  });
  await actions.open(message(), 'reactions');
  assert.equal(f.dismisses(), 1);
  const latest = actions.open(other, 'actions', undefined, undefined, measured.measure);
  dismissed.resolve(undefined);
  await old;
  await measured.entered;
  assert.equal(obsoleteMeasurements, 0);
  await actions.open(other, 'actions');
  assert.equal(f.dismisses(), 2, 'Old finally must retain the newer measurement latch');
  measured.resolve(settledAnchor);
  await latest;
  assert.equal(f.render().selectedId, other.id);
  assert.equal(f.render().panel.props.anchor, settledAnchor);
});

test('a different reaction chip supersedes the same message participant request during dismissal', async () => {
  const f = fixture();
  const dismissed = deferred<void>();
  f.dismiss(() => dismissed.promise);
  const first = f.actions.open(
    message(),
    'participants',
    undefined,
    '👍',
    async () => liftedAnchor,
  );
  const latest = f.actions.open(
    message(),
    'participants',
    undefined,
    '❤️',
    async () => settledAnchor,
  );
  assert.equal(f.dismisses(), 2);
  dismissed.resolve(undefined);
  await Promise.all([first, latest]);
  assert.equal(f.render().panel.props.selectedEmoji, '❤️');
});

test('pending overlay reads stay cancelled through transient session/access loss at either await', async () => {
  for (const stage of ['dismiss', 'measure'] as const) {
    for (const [name, change] of invalidSessions) {
      const f = fixture();
      const original = f.state();
      const dismissed = deferred<void>();
      const measured = anchorMeasurement();
      let measurements = 0;
      f.dismiss(() => dismissed.promise);
      const pending = f.actions.open(message(), 'reactions', undefined, undefined, () => {
        ++measurements;
        return measured.measure();
      });
      if (stage === 'measure') {
        dismissed.resolve(undefined);
        await measured.entered;
      }
      f.update(change);
      f.update(() => original);
      const writes = f.writes.length;
      dismissed.resolve(undefined);
      measured.resolve(settledAnchor);
      await pending;
      assert.equal(f.render().selectedId, null, `${stage}: ${name}`);
      assert.equal(f.writes.length, writes, `${stage}: ${name}`);
      assert.equal(measurements, stage === 'measure' ? 1 : 0, `${stage}: ${name}`);
      assert.deepEqual(f.alerts, []);
    }
  }
});

test('blur, unmount, close and another message action prevent pending measurement from reopening a menu', async () => {
  for (const action of [
    'blur',
    'unmount',
    'close',
    'reply',
    'thread',
    'profile',
    'react',
  ] as const) {
    const f = fixture();
    const measured = anchorMeasurement();
    const pending = f.actions.open(message(), 'reactions', undefined, undefined, measured.measure);
    await measured.entered;
    if (action === 'blur' || action === 'unmount') f[action]();
    else if (action === 'close') f.actions.close();
    else if (action === 'react') await f.actions.react(message(), '👍');
    else await f.actions[action](message());
    const writes = f.writes.length;
    measured.resolve(settledAnchor);
    await pending;
    assert.equal(f.writes.length, writes, action);
    assert.deepEqual(f.alerts, []);
    if (action !== 'unmount') assert.equal(f.render().selectedId, null, action);
  }
});

test('disabled/source round trips and missing/deleted entries detach a pending overlay measurement', async () => {
  for (const action of ['disabled', 'source', 'missing', 'deleted'] as const) {
    const f = fixture();
    f.update((state) => ({
      ...state,
      snapshot: {
        ...state.snapshot!,
        conversations: [
          ...state.snapshot!.conversations,
          { ...state.snapshot!.conversations[0]!, id: 'other_chat' },
        ],
      },
    }));
    const measured = anchorMeasurement();
    const pending = f.actions.open(message(), 'reactions', undefined, undefined, measured.measure);
    await measured.entered;
    if (action === 'disabled') {
      f.render({ disabled: true });
      f.render({ disabled: false });
    } else if (action === 'source') {
      f.render({ spaceId: 'other_chat' });
      f.render({ spaceId });
    } else f.render({ entries: action === 'missing' ? [] : [message({ deleted: true })] });
    measured.resolve(settledAnchor);
    await pending;
    assert.equal(f.render().selectedId, null, action);
  }
});

test('a newly opening keyboard cancels the measured overlay instead of hiding the new typing session', async () => {
  const f = fixture();
  await f.actions.open(message(), 'reactions', undefined, undefined, async () => {
    f.keyboard(true);
    return settledAnchor;
  });
  assert.equal(f.render().selectedId, null);
  assert.equal(f.dismisses(), 1);
});

test('dismissal and measurement failures allow retry while detached failures remain silent', async () => {
  for (const stage of ['dismiss', 'measure'] as const) {
    const f = fixture();
    const failure = new Error('Synthetic overlay positioning failure');
    if (stage === 'dismiss')
      f.dismiss(async () => {
        throw failure;
      });
    await f.actions.open(message(), 'actions', undefined, undefined, async () => {
      if (stage === 'measure') throw failure;
      return settledAnchor;
    });
    assert.equal(f.render().selectedId, null);
    assert.equal(f.alerts.length, 1);
    assert.equal(f.alerts[0]![0], 'chat.theMessageCouldNotBeOpened');
    f.dismiss(async () => {});
    await f.actions.open(message(), 'actions', undefined, undefined, async () => settledAnchor);
    assert.equal(f.render().selectedId, message().id);
    const stale = fixture();
    const wait = anchorMeasurement();
    const pending = stale.actions.open(message(), 'actions', undefined, undefined, wait.measure);
    await wait.entered;
    stale.blur();
    wait.reject(failure);
    await pending;
    assert.deepEqual(stale.alerts, []);
  }
});
