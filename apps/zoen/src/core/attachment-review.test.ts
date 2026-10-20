import { PlaybackCoordinator, type PlaybackOwner } from '../media/playback-owner.ts';
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

for (const failureAt of ['admission', 'completion', 'native-error'] as const) {
  test(`throwing review observers at ${failureAt} cannot retain a playback owner or mask native cancellation`, async (t) => {
    const removed: PlaybackOwner[] = [];
    const coordinator = new (class extends PlaybackCoordinator {
      override remove(owner: PlaybackOwner) {
        removed.push(owner);
        super.remove(owner);
      }
    })();
    const request = fixture().request;
    const presented = deferred<void>();
    const viewer = deferred<void>();
    const nativeFailure = Error('Native review decoder failed');
    const cancelFailure = Error('Native dismissal temporarily unavailable');
    let failCancel = failureAt === 'admission';
    let cancellations = 0;
    const review = new AttachmentReview(
      {
        prepare: async () => request,
        current: (selected) => selected === request,
        token: () => 'owned-review',
        preview: async () => {
          presented.resolve();
          await viewer.promise;
        },
        cancel: async (token) => {
          assert.equal(token, 'owned-review');
          cancellations++;
          if (failCancel) throw cancelFailure;
          viewer.resolve();
        },
      },
      coordinator,
    );
    let refusedNotifications = 0;
    const healthyStates: { mutationId: string | null; error: Error | null }[] = [];
    const offBad = review.subscribe(() => {
      const state = review.getSnapshot();
      if (
        (failureAt === 'admission' && state.mutationId !== null) ||
        (failureAt === 'completion' && state.mutationId === null) ||
        (failureAt === 'native-error' && state.error)
      ) {
        refusedNotifications++;
        throw Error('Synthetic review control notification failure');
      }
    });
    const offHealthy = review.subscribe(() => {
      healthyStates.push(review.getSnapshot());
    });
    t.after(async () => {
      offBad();
      offHealthy();
      failCancel = false;
      viewer.resolve();
      await review.close();
    });
    let opening!: Promise<void>;
    assert.doesNotThrow(() => {
      opening = review.open('intent');
    });
    void opening.catch(() => {});
    await presented.promise;
    assert.equal(removed.length, 0, 'Native presentation still owns playback exclusion');
    if (failureAt === 'admission') {
      await assert.rejects(review.close(), (error: unknown) => error === cancelFailure);
      assert.equal(removed.length, 0, 'Failed dismissal retains ownership until the viewer drains');
      failCancel = false;
      await review.close();
      assert.equal(cancellations, 2);
    } else if (failureAt === 'native-error') viewer.reject(nativeFailure);
    else viewer.resolve();
    await opening;
    assert.ok(refusedNotifications > 0);
    assert.equal(removed.length, 1, 'Exact real coordinator ownership must be removed');
    assert.deepEqual(healthyStates.at(-1), {
      mutationId: null,
      error: failureAt === 'native-error' ? nativeFailure : null,
    });
    assert.equal(review.getSnapshot().mutationId, null);
    await review.close();
    assert.equal(removed.length, 1, 'Repeated Close cannot release another playback owner');
  });
}
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

test('opening native review interrupts received audio and never reclaims a newer play intent after delayed preparation', async () => {
  const coordinator = new PlaybackCoordinator();
  let pauses = 0,
    playing = true;
  const audio = coordinator.register(
    () => {
      pauses++;
      playing = false;
    },
    () => playing,
  );
  audio.begin();
  const preparation = deferred<AttachmentReviewRequest | null>();
  let opened = 0;
  const request = fixture().request;
  const review = new AttachmentReview(
    {
      prepare: () => preparation.promise,
      current: () => true,
      token: () => 'preview',
      preview: async () => {
        opened++;
      },
      cancel: async () => {},
    },
    coordinator,
  );
  const pending = review.open('intent');
  await tick();
  assert.equal(pauses, 1);
  assert.equal(playing, false);
  const newer = coordinator.register(
    () => {},
    () => false,
  );
  const intent = newer.begin();
  preparation.resolve(request);
  await pending;
  assert.equal(opened, 0);
  assert.equal(newer.isCurrent(intent), true);
  audio.dispose();
  newer.dispose();
});

test('a newer playback intent dismisses the exact open native viewer before its lease is released', async () => {
  const coordinator = new PlaybackCoordinator(),
    viewer = deferred<void>();
  const request = fixture().request,
    tokens: string[] = [];
  const review = new AttachmentReview(
    {
      prepare: async () => request,
      current: () => true,
      token: () => 'viewer',
      preview: async () => viewer.promise,
      cancel: async (token) => {
        tokens.push(token);
      },
    },
    coordinator,
  );
  const pending = review.open('intent');
  await tick();
  const newer = coordinator.register(
      () => {},
      () => false,
    ),
    intent = newer.begin();
  await tick();
  assert.deepEqual(tokens, ['viewer']);
  assert.equal(review.getSnapshot().mutationId, 'intent');
  viewer.resolve();
  await pending;
  assert.equal(review.getSnapshot().mutationId, null);
  assert.equal(newer.isCurrent(intent), true);
  newer.dispose();
});
