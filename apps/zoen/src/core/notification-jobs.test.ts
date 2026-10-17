import assert from 'node:assert/strict';
import test from 'node:test';
import { NotificationJobs } from '../notifications/jobs.ts';

test('notification cleanup waits for delayed schedules and failed badge writes before clearing', async () => {
  const jobs = new NotificationJobs();
  const events: string[] = [];
  let scheduled!: () => void;
  jobs.track(
    new Promise<void>((resolve) => {
      scheduled = resolve;
    }).then(() => {
      events.push('scheduled');
    }),
  );
  jobs.track(Promise.reject(new Error('badge unavailable')));
  const cleanup = jobs.drain().then(() => {
    events.push('cleared');
  });
  await Promise.resolve();
  assert.deepEqual(events, []);
  scheduled();
  await cleanup;
  assert.deepEqual(events, ['scheduled', 'cleared']);
  await jobs.drain();
});
