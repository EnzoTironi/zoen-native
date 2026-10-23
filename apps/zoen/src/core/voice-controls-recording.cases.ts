import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { VoiceSession } from '../media/voice-session.ts';
import { deferred, find, nodes, recording, tick } from './voice-controls.fixtures.ts';

test('recording pause/resume retains one prepared capture and finalizes it once into the shared preview', async () => {
  const f = recording(true);
  await find(f.render(), 'VoiceRecordButton').props.onStart(false);
  let dock = find(f.render(), 'VoiceDock');
  assert.equal(dock.props.recording, true);
  assert.equal(dock.props.locked, true);
  await dock.props.onPause();
  dock = find(f.render(), 'VoiceDock');
  assert.equal(f.state.isRecording, false);
  assert.equal(dock.props.recording, true, 'paused audio still owns the unfinished native capture');
  assert.equal(dock.props.paused, true);
  assert.equal(find(f.render(), 'VoiceRecordButton').props.disabled, true);
  assert.equal(f.prepares(), 1);
  assert.equal(f.stops(), 0);
  await dock.props.onPause();
  assert.equal(f.state.isRecording, true);
  assert.equal(f.prepares(), 1, 'resume must not prepare a second source');
  assert.equal(f.records(), 2);
  await find(f.render(), 'VoiceDock').props.onStop();
  await tick();
  assert.equal(f.stops(), 1);
  const tree = f.render();
  const session = find(tree, 'VoicePreviewPlayback').props.session as VoiceSession;
  assert.ok(session.getSendable());
  assert.strictEqual(find(tree, 'VoiceDock').props.session, session);
  assert.deepEqual(f.sent, []);
  await find(tree, 'VoiceDock').props.onCancel();
});

test('suspending a paused capture finalizes its same draft without sending or discarding the source', async () => {
  const f = recording(true);
  await find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await find(f.render(), 'VoiceDock').props.onPause();
  f.render({ suspended: true });
  await tick();
  const tree = f.render({ suspended: true });
  assert.equal(f.stops(), 1);
  assert.equal(f.prepares(), 1);
  assert.deepEqual(f.sent, []);
  assert.deepEqual(f.discarded, []);
  assert.ok(find(tree, 'VoicePreviewPlayback').props.session?.getSendable());
  await find(f.render(), 'VoiceDock').props.onCancel();
});

for (const editable of [false, true]) {
  test(`retained Cancel from draft A cannot discard draft B; fresh Cancel still completes (${editable ? 'edited' : 'raw'})`, async () => {
    const f = recording(editable);
    await find(f.render(), 'VoiceRecordButton').props.onStart(false);
    await find(f.render(), 'VoiceDock').props.onStop();
    await tick();
    const old = find(f.render(), 'VoiceDock').props;
    await old.onCancel();

    await find(f.render(), 'VoiceRecordButton').props.onStart(false);
    await find(f.render(), 'VoiceDock').props.onStop();
    await tick();
    const current = find(f.render(), 'VoiceDock').props;
    const session = current.session as VoiceSession | undefined;
    const beforeDiscard = f.discarded.length;
    await old.onCancel();
    if (old.onRetry) await old.onRetry();
    old.onExpand();

    assert.equal(f.discarded.length, beforeDiscard, 'stale Cancel must not delete B');
    assert.strictEqual(find(f.render(), 'VoiceDock').props.session, session);
    assert.equal(
      find(f.render(), 'MediaSheet').props.visible,
      false,
      'stale Expand cannot present B',
    );
    if (session) assert.equal(session.getSnapshot().phase, 'ready');
    assert.deepEqual(f.sent, []);

    await current.onCancel();
    assert.equal(
      nodes(f.render()).some((node) => node.type === 'VoiceDock'),
      false,
    );
    if (session) assert.equal(session.getSnapshot().phase, 'closed');
  });
}

test('retained recording controls and microphone callbacks cannot stop, pause, cancel or restart a later capture', async () => {
  const f = recording(true);
  const oldMic = find(f.render(), 'VoiceRecordButton').props;
  await oldMic.onStart(false);
  const oldDock = find(f.render(), 'VoiceDock').props;
  await oldDock.onStop();
  await tick();
  await find(f.render(), 'VoiceDock').props.onCancel();

  await find(f.render(), 'VoiceRecordButton').props.onStart(false);
  const current = find(f.render(), 'VoiceDock').props;
  const stops = f.stops();
  const pauses = f.events.filter((event) => event.startsWith('pause:')).length;
  await oldDock.onPause();
  await oldDock.onStop();
  await oldDock.onCancel();
  await oldMic.onReview();
  await oldMic.onCancel();
  assert.equal(await oldMic.onStart(false), false);
  assert.equal(f.stops(), stops, 'stale Stop/Review cannot finalize B');
  assert.equal(f.events.filter((event) => event.startsWith('pause:')).length, pauses);
  assert.equal(f.state.isRecording, true);
  assert.equal(find(f.render(), 'VoiceDock').props.paused, false);

  await current.onPause();
  assert.equal(f.state.isRecording, false, 'fresh Pause still addresses B');
  await find(f.render(), 'VoiceDock').props.onStop();
  await tick();
  await find(f.render(), 'VoiceDock').props.onCancel();
});

test('Cancel admitted during recording still drains Stop and discards its own newly prepared review', async () => {
  const f = recording(true);
  await find(f.render(), 'VoiceRecordButton').props.onStart(false);
  const restoration = deferred<void>();
  f.waitRestore(restoration.promise);
  const admitted = find(f.render(), 'VoiceDock').props.onCancel();
  await tick();
  assert.equal(f.stops(), 1);
  assert.equal(f.state.isRecording, false);
  restoration.resolve();
  await admitted;
  assert.equal(
    nodes(f.render()).some((node) => node.type === 'VoiceDock'),
    false,
  );
  assert.deepEqual(f.sent, []);
  assert.ok(f.discarded.includes('file:///voice/capture.m4a'));
});
