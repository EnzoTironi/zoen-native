import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  deferred,
  find,
  interrupt,
  interruptions,
  nodes,
  recording,
  tick,
} from './voice-controls.fixtures.ts';

test('a system-paused recorder is finalized before its retained voice is prepared', async () => {
  const f = recording(true);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.pauseRecorder();
  f.background();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(f.stops(), 1);
  assert.ok(f.events.indexOf('stop:capture') < f.events.indexOf('copy'));
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  assert.deepEqual(f.sent, [], 'interruption retains a draft and never posts it');
  f.background('active');
  await dock.props.onSend(dock.props.session.getSendable());
  assert.deepEqual(f.sent, [4000]);
});

test('the Web recorder URI published only by stop is retained as one sendable review', async () => {
  const f = recording(false, 'web');
  f.publishUriOnStop();
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  const recordingDock = find(f.render(), 'VoiceDock');
  assert.equal(recordingDock.props.uri, undefined);
  await recordingDock.props.onStop();
  const tree = f.render();
  assert.equal(nodes(tree).filter((node) => node.type === 'VoiceDock').length, 1);
  const review = find(tree, 'VoiceDock');
  assert.equal(review.props.uri, 'file:///voice/capture.m4a');
  assert.equal(review.props.durationMs, 4000);
  assert.equal(f.stops(), 1);
  await review.props.onSend();
  assert.deepEqual(f.sent, [4000]);
});

test('unsupported Web prepared cancellation cannot create a shared microphone lock for a new route', async () => {
  const f = recording(false, 'web');
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  interrupt(f, 'focus');
  prepare.resolve();
  await tick();
  assert.equal(f.records(), 0);
  assert.ok(f.stops() > 0, 'the actual Web contract rejects stop before ever recording');
  const next = f.nextMount();
  // Keep the failed owner alive: another Web route still owns an independent
  // MediaRecorder, even before the old route has unmounted or cleaned up.
  find(next.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  assert.equal(next.state.isRecording, true);
  assert.equal(f.records(), 1);
  f.releaseRecorder();
  f.app.unmount();
  await find(next.render(), 'VoiceDock').props.onCancel();
  next.unmount();
});

test('failed preparation restores the session without finalizing an unprepared recorder or locking retry', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  prepare.reject(new Error('synthetic prepare failure'));
  await tick();
  assert.equal(f.records(), 0);
  assert.equal(f.stops(), 0);
  assert.equal(f.events.filter((event) => event === 'mode:false').length, 1);
  f.waitPrepare(undefined);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  assert.equal(f.records(), 1);
  await find(f.render(), 'VoiceDock').props.onCancel();
});

test('failed paused finalization retains ownership and retries before editor copy', async () => {
  const f = recording(true);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.pauseRecorder();
  f.failStop(true);
  f.background();
  await tick();
  assert.equal(f.events.includes('copy'), false);
  assert.deepEqual(f.discarded, []);
  f.failStop(false);
  f.background('active');
  find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(f.stops(), 2);
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  await dock.props.onCancel();
});

test('post-stop status failure retains original duration and does not stop twice on retry', async () => {
  const f = recording(true);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.failConfirmation();
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /confirmation failure/);
  assert.equal(f.state.durationMillis, 0, 'the real SDK no longer exposes pre-stop duration');
  find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(dock.props.durationMs, 4000);
  assert.equal(f.stops(), 1);
  await dock.props.onCancel();
});

test('a failed initial status read keeps the source owned for a later successful stop', async () => {
  const f = recording(true);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.failStatus();
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /initial status failure/);
  assert.equal(f.stops(), 0);
  assert.equal(f.events.includes('copy'), false);
  assert.deepEqual(f.discarded, []);
  find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(dock.props.durationMs, 4000);
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  await dock.props.onCancel();
});

test('a resolved stop with native capture still active re-arms finalization for a real retry', async () => {
  const f = recording(true);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.ineffectiveStop();
  await assert.rejects(
    find(f.render(), 'VoiceDock').props.onStop(),
    /media.theRecordingOnThisDeviceCouldNotBe/,
  );
  assert.equal(f.events.includes('copy'), false);
  assert.equal(f.state.isRecording, true);
  await find(f.render(), 'VoiceDock').props.onRetry();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(f.stops(), 2);
  assert.equal(f.state.isRecording, false);
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  assert.equal(dock.props.durationMs, 4000);
  await dock.props.onCancel();
});

test('two microphone presses before React commits admit only one permission and preparation', async () => {
  const f = recording();
  const permission = deferred<{ granted: boolean }>();
  f.waitPermission(permission.promise);
  const mic = find(f.render(), 'VoiceRecordButton');
  mic.props.onStart(false);
  mic.props.onStart(false);
  assert.equal(find(f.render(), 'VoiceRecordButton').props.disabled, true);
  await tick();
  assert.equal(f.permissionCalls(), 1);
  permission.resolve({ granted: true });
  await tick();
  assert.equal(f.prepares(), 1);
  assert.equal(f.records(), 1);
  await find(f.render(), 'VoiceDock').props.onCancel();
});
for (const kind of interruptions) {
  test(`permission pending across ${kind} cannot revive without a fresh microphone press`, async () => {
    const f = recording();
    const permission = deferred<{ granted: boolean }>();
    f.waitPermission(permission.promise);
    find(f.render(), 'VoiceRecordButton').props.onStart(false);
    await tick();
    interrupt(f, kind);
    permission.resolve({ granted: true });
    await tick();
    assert.equal(f.prepares(), 0);
    assert.equal(f.records(), 0);
    assert.equal(f.events.includes('mode:true'), false);
    find(f.render(), 'VoiceRecordButton').props.onStart(false);
    await tick();
    assert.equal(f.records(), 1, 'a new explicit intent can record after cleanup');
    await find(f.render(), 'VoiceDock').props.onCancel();
  });
  test(`native preparation pending across ${kind} finalizes and discards instead of starting`, async () => {
    const f = recording();
    const prepare = deferred<void>();
    f.waitPrepare(prepare.promise);
    find(f.render(), 'VoiceRecordButton').props.onStart(false);
    await tick();
    assert.equal(f.prepares(), 1);
    interrupt(f, kind);
    prepare.resolve();
    await tick();
    assert.equal(f.records(), 0);
    assert.equal(f.stops(), 1);
    assert.deepEqual(f.discarded, ['file:///voice/capture.m4a']);
    assert.equal(f.events.filter((event) => event === 'mode:false').length, 1);
    assert.ok(f.events.indexOf('stop:capture') < f.events.indexOf('mode:false'));
    find(f.render(), 'VoiceRecordButton').props.onStart(false);
    await tick();
    assert.equal(f.records(), 1);
    await find(f.render(), 'VoiceDock').props.onCancel();
  });
}

test('unmount during preparation drains audio restoration without touching the released SharedObject', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  const cleanup = [...f.cleanups][0]!;
  f.releaseRecorder();
  f.app.unmount();
  const drained = cleanup();
  prepare.resolve();
  await drained;
  assert.equal(f.records(), 0);
  assert.equal(f.releasedCalls(), 0);
  assert.equal(f.events.filter((event) => event === 'mode:false').length, 1);
});

test('unmount while permission is pending never prepares or restores an unused audio session', async () => {
  const f = recording();
  const permission = deferred<{ granted: boolean }>();
  f.waitPermission(permission.promise);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.releaseRecorder();
  f.app.unmount();
  permission.resolve({ granted: true });
  await tick();
  assert.equal(f.prepares(), 0);
  assert.equal(f.records(), 0);
  assert.equal(f.releasedCalls(), 0);
  assert.equal(f.events.includes('mode:true'), false);
  assert.equal(f.events.includes('mode:false'), false);
});

for (const intent of ['device cleanup', 'Cancel'] as const) {
  test(`${intent} during native preparation retires the pending microphone intent before drain`, async () => {
    const f = recording(true);
    const prepare = deferred<void>();
    f.waitPrepare(prepare.promise);
    find(f.render(), 'VoiceRecordButton').props.onStart(false);
    await tick();
    let cleanup: Promise<void> | undefined;
    if (intent === 'device cleanup') cleanup = [...f.cleanups][0]!();
    else find(f.render(), 'IconButton', 'media.cancelAttachment').props.onPress();
    prepare.resolve();
    await cleanup;
    await tick();
    assert.equal(f.records(), 0);
    assert.equal(f.stops(), 1);
    assert.equal(f.events.includes('copy'), false);
    assert.ok(f.discarded.includes('file:///voice/capture.m4a'));
    assert.equal(
      nodes(f.render()).some((node) => node.type === 'VoiceDock'),
      false,
    );
    find(f.render(), 'VoiceRecordButton').props.onStart(false);
    await tick();
    assert.equal(f.records(), 1, 'only a new explicit press can start after cancellation');
    await find(f.render(), 'VoiceDock').props.onCancel();
  });
}

test('a failed same-composer restoration can recover without deadlocking its next recording', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.failRestore(true);
  interrupt(f, 'focus');
  prepare.resolve();
  await tick();
  assert.equal(f.records(), 0);
  assert.equal(find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.disabled, false);
  f.failRestore(false);
  find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  assert.equal(f.records(), 1);
  await find(f.render(), 'VoiceDock').props.onCancel();
});

test('new route explicitly recovers departed cleanup before admission and exposes pending immediately', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.failRestore(true);
  f.releaseRecorder();
  f.app.unmount();
  prepare.resolve();
  await tick();
  const next = f.nextMount();
  const restore = deferred<void>();
  f.waitRestore(restore.promise);
  f.failRestore(false);
  const mic = find(next.render(), 'VoiceRecordButton');
  mic.props.onStart(false);
  mic.props.onStart(false);
  assert.equal(find(next.render(), 'VoiceRecordButton').props.disabled, true);
  await tick();
  assert.equal(f.permissionCalls(), 1, 'no new preparation can overtake an old restore');
  restore.resolve();
  await tick();
  assert.equal(f.records(), 1);
  assert.equal(next.state.isRecording, true);
  assert.equal(f.releasedCalls(), 0);
  const newRecord = f.events.indexOf('record:next');
  await tick();
  assert.equal(f.events.slice(newRecord + 1).includes('mode:false'), false);
  await find(next.render(), 'VoiceDock').props.onCancel();
  next.unmount();
});

test('new route cleanup failure stays retryable through its own visible recovery control', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.failRestore(true);
  f.releaseRecorder();
  f.app.unmount();
  prepare.resolve();
  await tick();
  const next = f.nextMount();
  find(next.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  assert.equal(f.records(), 0);
  const retry = find(next.render(), 'NativeButton', 'media.voiceTryAgain');
  assert.equal(
    retry.props.disabled,
    false,
    'a departed owner cannot leave an opaque permanent lock',
  );
  f.failRestore(false);
  retry.props.onPress();
  await tick();
  find(next.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  assert.equal(f.records(), 1);
  assert.equal(f.releasedCalls(), 0);
  await find(next.render(), 'VoiceDock').props.onCancel();
  next.unmount();
});

test('Send cannot bypass failed audio restoration and retries preserve the exact capture', async () => {
  for (const editable of [false, true]) {
    const f = recording(editable);
    find(f.render(), 'VoiceRecordButton').props.onStart(false);
    await tick();
    f.failRestore(true);
    await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /restore failure/);
    await tick();
    const props = find(f.render(), 'VoiceDock').props;
    const capture = props.session?.getSendable();
    const pending = props.onSend(capture);
    if (editable) await assert.rejects(pending, /restore failure/);
    else await pending;
    assert.deepEqual(f.sent, []);
    assert.deepEqual(f.discarded, []);
    assert.equal(f.stops(), 1);
    if (editable) assert.strictEqual(props.session.getSendable().preview, capture.preview);
    f.failRestore(false);
    await props.onSend(capture);
    assert.deepEqual(f.sent, [4000]);
    assert.equal(f.stops(), 1);
    await tick();
  }
});
