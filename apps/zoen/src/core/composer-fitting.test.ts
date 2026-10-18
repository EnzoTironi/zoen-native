import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  createComposerFitting,
  type ComposerFittingMeasurement,
  type ComposerFittingRequest,
  type ComposerFittingSnapshot,
} from './composer-fitting.ts';

const snapshot = (patch: Partial<ComposerFittingSnapshot> = {}): ComposerFittingSnapshot =>
  Object.freeze({
    text: 'Synthetic draft',
    label: 'Message',
    placeholder: 'Message',
    width: 264,
    fontSize: 16,
    context: 'normal:1',
    tag: 101,
    ...patch,
  });
const drain = () => new Promise<void>((done) => setImmediate(done));
function fixture() {
  const reads: {
    request: ComposerFittingRequest;
    resolve(value: ComposerFittingMeasurement | null): void;
    reject(reason: unknown): void;
  }[] = [];
  const applied: { height: number; snapshot: ComposerFittingSnapshot }[] = [];
  const frames: { run(): void; cancelled: boolean }[] = [];
  const fitting = createComposerFitting(
    (request) =>
      new Promise((resolve, reject) => {
        reads.push({ request, resolve, reject });
      }),
    (height, snapshot) => applied.push({ height, snapshot }),
    (run) => {
      const frame = { run, cancelled: false };
      frames.push(frame);
      return () => {
        frame.cancelled = true;
      };
    },
  );
  return {
    fitting,
    reads,
    applied,
    frames,
    accept: (index: number, patch: Partial<ComposerFittingMeasurement> = {}) => {
      const read = reads[index]!;
      read.resolve({
        tag: read.request.tag,
        nonce: read.request.nonce,
        width: read.request.width,
        fontSize: read.request.fontSize,
        height: 74,
        ...patch,
      });
    },
  };
}

test('applies native fitting to the exact original snapshot and bounds height without adding padding', async () => {
  for (const [native, expected] of [
    [31, 52],
    [74.5, 74.5],
    [220, 160],
  ]) {
    const f = fixture();
    const value = snapshot();
    f.fitting.request(value);
    f.accept(0, { height: native });
    await drain();
    assert.equal(f.applied[0]!.height, expected);
    assert.equal(f.applied[0]!.snapshot, value);
    assert.notEqual(f.reads[0]!.request, value, 'nonce belongs to a native request clone');
    assert.equal(f.reads[0]!.request.placeholder, value.placeholder);
  }
});

test('new text, mode, label, placeholder, tag, width or font invalidates older async measurements', async () => {
  for (const patch of [
    { text: 'New typing' },
    { context: 'edit:2' },
    { label: 'Edit message' },
    { placeholder: 'Search this chat' },
    { tag: 202 },
    { width: 310 },
    { fontSize: 21 },
  ]) {
    const f = fixture();
    const latest = snapshot(patch);
    f.fitting.request(snapshot());
    f.fitting.request(latest);
    f.accept(1);
    await drain();
    f.accept(0, { height: 150 });
    await drain();
    assert.equal(f.applied.length, 1);
    assert.equal(f.applied[0]!.snapshot, latest);
    assert.equal(f.applied[0]!.height, 74);
  }
});

test('a same-snapshot content-size notification still fences older requests and stale rejection cannot schedule a retry', async () => {
  const f = fixture();
  const value = snapshot();
  f.fitting.request(value);
  f.fitting.request(value);
  assert.notEqual(f.reads[0]!.request.nonce, f.reads[1]!.request.nonce);
  f.reads[0]!.reject(new Error('Late old native failure'));
  await drain();
  assert.equal(f.frames.length, 0);
  f.accept(1);
  await drain();
  assert.equal(f.applied.length, 1);
});

test('missing layout and rejected native reads retry on at most two injected animation frames', async () => {
  const f = fixture();
  f.fitting.request(snapshot());
  f.reads[0]!.resolve(null);
  await drain();
  assert.equal(f.frames.length, 1);
  assert.equal(f.reads.length, 1);
  f.frames[0]!.run();
  f.reads[1]!.reject(new Error('Native view not ready'));
  await drain();
  assert.equal(f.frames.length, 2);
  assert.equal(f.reads.length, 2);
  f.frames[1]!.run();
  f.reads[2]!.resolve(null);
  await drain();
  assert.equal(f.frames.length, 2);
  assert.equal(f.reads.length, 3);
  assert.equal(new Set(f.reads.map((read) => read.request.nonce)).size, 3);
  assert.equal(f.applied.length, 0);
});

test('mismatched or invalid native result fields cannot affect height and a valid retry can recover', async () => {
  for (const patch of [
    { tag: 999 },
    { nonce: -1 },
    { width: 266 },
    { width: NaN },
    { width: 0 },
    { fontSize: 17 },
    { fontSize: Infinity },
    { fontSize: 0 },
    { height: NaN },
    { height: Infinity },
    { height: 0 },
    { height: -1 },
  ]) {
    const f = fixture();
    const value = snapshot();
    f.fitting.request(value);
    f.accept(0, patch);
    await drain();
    assert.equal(f.applied.length, 0);
    assert.equal(f.frames.length, 1);
    f.frames[0]!.run();
    f.accept(1, { width: 264.5, fontSize: 16.25 });
    await drain();
    assert.equal(f.applied.length, 1);
    assert.equal(f.applied[0]!.snapshot, value);
  }
});

test('invalid snapshots cancel pending measurements and scheduled retries', async () => {
  for (const patch of [
    { width: 0 },
    { width: NaN },
    { fontSize: 0 },
    { tag: null },
    { tag: -1 },
    { tag: 1.5 },
    { label: '' },
  ]) {
    const f = fixture();
    f.fitting.request(snapshot());
    f.fitting.request(snapshot(patch));
    assert.equal(f.reads.length, 1);
    f.accept(0);
    await drain();
    assert.equal(f.applied.length, 0);
    f.fitting.request(snapshot());
    f.reads[1]!.resolve(null);
    await drain();
    f.fitting.request(snapshot(patch));
    assert.equal(f.frames[0]!.cancelled, true);
    f.frames[0]!.run();
    assert.equal(f.reads.length, 2, 'even an undelivered frame cancellation is fenced');
  }
});

test('cancel permits StrictMode setup again while dispose permanently rejects pending and future work', async () => {
  const f = fixture();
  const value = snapshot();
  f.fitting.request(value);
  f.fitting.cancel();
  f.accept(0);
  await drain();
  assert.equal(f.applied.length, 0);
  f.fitting.request(value);
  f.accept(1);
  await drain();
  assert.equal(f.applied.length, 1);
  f.fitting.request(value);
  f.reads[2]!.resolve(null);
  await drain();
  f.fitting.dispose();
  assert.equal(f.frames[0]!.cancelled, true);
  f.frames[0]!.run();
  f.fitting.request(value);
  assert.equal(f.reads.length, 3);
  assert.equal(f.applied.length, 1);
});

test('dispose also suppresses a successful native result already in flight', async () => {
  const f = fixture();
  f.fitting.request(snapshot());
  f.fitting.dispose();
  f.accept(0);
  await drain();
  assert.equal(f.applied.length, 0);
});
