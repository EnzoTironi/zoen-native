import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AttachmentReview, type AttachmentReviewRequest } from '../media/attachment-review.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
const tick = async () => {
  for (let index = 0; index < 8; index++) await Promise.resolve();
};
function fixture() {
  let sequence = 0;
  let available = true;
  const request: AttachmentReviewRequest = Object.freeze({
    scope: Object.freeze({
      expected_identity_id: 'owner',
      expected_device_id: 'device',
      space_id: 'chat',
      thread_root_id: 'root',
    }),
    revision: 'saved',
    assetId: 'asset',
    mutationId: 'intent',
    sha256: 'a'.repeat(64),
    replyTo: 'root',
  });
  let preparation: Promise<AttachmentReviewRequest | null> = Promise.resolve(request);
  const opened: { request: AttachmentReviewRequest; token: string }[] = [];
  const canceled: string[] = [];
  const viewers = new Map<string, ReturnType<typeof deferred<void>>>();
  let acknowledgeCancel = true;
  const review = new AttachmentReview({
    prepare: async (mutationId) => {
      assert.equal(mutationId, 'intent');
      return preparation;
    },
    current: (value) => available && value === request,
    token: () => `preview-${++sequence}`,
    preview: async (value, token) => {
      opened.push({ request: value, token });
      const viewer = deferred<void>();
      viewers.set(token, viewer);
      await viewer.promise;
    },
    cancel: async (token) => {
      canceled.push(token);
      if (acknowledgeCancel) viewers.get(token)?.resolve();
    },
  });
  return {
    review,
    request,
    opened,
    canceled,
    prepare: (value: Promise<AttachmentReviewRequest | null>) => {
      preparation = value;
    },
    invalidate: () => {
      available = false;
      review.refresh();
    },
    delayDismissal: () => {
      acknowledgeCancel = false;
    },
    dismiss: () => viewers.get(opened.at(-1)!.token)!.resolve(),
    fail: (reason: unknown) => viewers.get(opened.at(-1)!.token)!.reject(reason),
  };
}

test('review borrows exact canonical selection and repeated presses share one presentation', async () => {
  const f = fixture();
  const first = f.review.open('intent');
  assert.equal(f.review.open('intent'), first);
  await tick();
  assert.equal(f.opened.length, 1);
  assert.equal(f.opened[0]!.request, f.request);
  assert.equal(f.review.getSnapshot().mutationId, 'intent');
  assert.equal('uri' in f.opened[0]!.request, false, 'native admission receives no arbitrary URI');
  f.dismiss();
  await first;
  assert.deepEqual(f.review.getSnapshot(), { mutationId: null, error: null });
  assert.deepEqual(f.canceled, []);
});

test('close waits for the viewer dismissal before the caller releases a draft preview copy', async () => {
  const f = fixture();
  f.delayDismissal();
  const opening = f.review.open('intent');
  await tick();
  let released = false;
  const closing = f.review.close().then(() => {
    released = true;
  });
  await tick();
  assert.deepEqual(f.canceled, ['preview-1']);
  assert.equal(released, false);
  f.dismiss();
  await Promise.all([opening, closing]);
  assert.equal(released, true);
});

test('late preparation after close cannot open any native viewer or change its destination', async () => {
  const f = fixture();
  const prepared = deferred<AttachmentReviewRequest | null>();
  f.prepare(prepared.promise);
  const opening = f.review.open('intent');
  await tick();
  const closing = f.review.close();
  prepared.resolve(f.request);
  await Promise.all([opening, closing]);
  assert.deepEqual(f.opened, []);
  assert.deepEqual(f.canceled, []);
  assert.equal(f.request.scope.thread_root_id, 'root');
  assert.equal(f.request.replyTo, 'root');
});

test('removal, session revocation or destination replacement closes the admitted presentation', async () => {
  const f = fixture();
  const opening = f.review.open('intent');
  await tick();
  f.invalidate();
  await opening;
  assert.deepEqual(f.canceled, ['preview-1']);
  await f.review.open('intent');
  assert.equal(f.opened.length, 1, 'an invalid selection must not reopen');
});

test('a failed native preview can retry the same draft intent with a fresh presentation token', async () => {
  const f = fixture();
  const first = f.review.open('intent');
  await tick();
  const failure = new Error('Native format unavailable');
  f.fail(failure);
  await first;
  assert.equal(f.review.getSnapshot().error, failure);
  const retry = f.review.open('intent');
  await tick();
  assert.equal(f.opened.length, 2);
  assert.notEqual(f.opened[0]!.token, f.opened[1]!.token);
  assert.equal(f.opened[1]!.request, f.request);
  assert.equal(f.review.getSnapshot().error, null);
  f.dismiss();
  await retry;
  assert.equal(f.review.getSnapshot().error, null);
});

test('close before the deferred admission starts does not even prepare a native preview', async () => {
  const f = fixture();
  const opening = f.review.open('intent');
  await f.review.close();
  await opening;
  assert.deepEqual(f.opened, []);
  assert.deepEqual(f.canceled, []);
});
