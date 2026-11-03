import assert from 'node:assert/strict';
import { test } from 'node:test';
import { DialogChoiceQueue } from '../components/dialog-choice-queue.ts';

const options = {
  title: 'Delete?',
  choices: [
    { id: 'for_me', label: 'On this device' },
    { id: 'everyone', label: 'For everyone' },
  ],
};

test('native dismissal after a choice cannot cancel the following request', async () => {
  const queue = new DialogChoiceQueue();
  const first = queue.choose(options),
    firstId = queue.getSnapshot()!.id;
  const second = queue.choose(options);
  queue.finish(firstId, 'for_me');
  queue.finish(firstId, null);
  assert.equal(await first, 'for_me');
  const next = queue.getSnapshot()!;
  assert.notEqual(next.id, firstId);
  queue.finish(next.id, 'everyone');
  assert.equal(await second, 'everyone');
  assert.equal(queue.getSnapshot(), null);
});

test('aborting either a queued or visible confirmation resolves cancel and retires its callbacks', async () => {
  const queue = new DialogChoiceQueue();
  const active = new AbortController(),
    queued = new AbortController();
  const first = queue.choose({ ...options, signal: active.signal });
  const firstId = queue.getSnapshot()!.id;
  const second = queue.choose({ ...options, signal: queued.signal });
  queued.abort();
  assert.equal(await second, null);
  assert.equal(queue.getSnapshot()!.id, firstId);
  active.abort();
  assert.equal(await first, null);
  queue.finish(firstId, 'everyone');
  assert.equal(queue.getSnapshot(), null);
  assert.equal(await queue.choose({ ...options, signal: active.signal }), null);
});

test('provider retirement cancels every request, rejects late use, and supports effect reactivation', async () => {
  const queue = new DialogChoiceQueue();
  let updates = 0;
  const detach = queue.subscribe(() => updates++);
  const first = queue.choose(options),
    second = queue.choose(options);
  queue.retire();
  assert.deepEqual(await Promise.all([first, second]), [null, null]);
  assert.equal(await queue.choose(options), null);
  detach();
  const before = updates;
  queue.activate();
  const third = queue.choose(options);
  queue.finish(queue.getSnapshot()!.id, 'forged-choice');
  assert.equal(await third, null);
  assert.equal(updates, before);
});
