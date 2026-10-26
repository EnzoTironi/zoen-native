import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  MINIAPP_REQUEST_TIMEOUT,
  waitForMiniappInitialization,
} from './protocol-initialization.ts';

test('missing bridge readiness terminates at the existing RPC deadline', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const initialization = new Promise<void>(() => {});
  const waiting = assert.rejects(waitForMiniappInitialization(initialization), /not initialized/);
  t.mock.timers.tick(MINIAPP_REQUEST_TIMEOUT);
  await waiting;
});

test('official readiness or disposal signal cancels the timer without advancing its deadline', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  let signal = () => {};
  const initialization = new Promise<void>((resolve) => {
    signal = resolve;
  });
  const waiting = waitForMiniappInitialization(initialization);
  signal();
  await waiting;
  t.mock.timers.tick(MINIAPP_REQUEST_TIMEOUT);
  await waiting;
});
