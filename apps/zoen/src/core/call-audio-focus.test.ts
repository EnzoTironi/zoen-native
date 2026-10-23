import assert from 'node:assert/strict';
import test from 'node:test';
import { CallAudioFocus } from '../media/call-audio-focus.ts';

const deferred = () => {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
};

test('call focus drains an already admitted audio mode change and rejects new ones', async () => {
  const focus = new CallAudioFocus();
  const old = deferred();
  const entered = deferred();
  const work = focus.run(async () => {
    entered.resolve();
    await old.promise;
  });
  await entered.promise;
  focus.begin('call-A');
  let drained = false;
  const drain = focus.drain().then(() => {
    drained = true;
  });
  await assert.rejects(
    focus.run(async () => {}),
    /Audio is in use/,
  );
  await Promise.resolve();
  assert.equal(drained, false);
  old.resolve();
  await work;
  await drain;
  assert.equal(drained, true);
  assert.equal(focus.available, false);
});

test('queued mode changes never start after call focus is synchronously fenced', async () => {
  const focus = new CallAudioFocus();
  let effects = 0;
  const late = focus.run(async () => {
    effects++;
  });
  focus.begin('call-A');
  await assert.rejects(late, /Audio is in use/);
  assert.equal(effects, 0);
});

test('old call release cannot retire newer audio ownership', async () => {
  const focus = new CallAudioFocus();
  focus.begin('call-A');
  assert.throws(() => focus.begin('call-B'), /another call/);
  assert.equal(focus.end('call-A'), true);
  focus.begin('call-B');
  assert.equal(focus.end('call-A'), false);
  assert.equal(focus.owns('call-B'), true);
  assert.equal(focus.end('call-B'), true);
  assert.equal(await focus.run(async () => 42), 42);
});
