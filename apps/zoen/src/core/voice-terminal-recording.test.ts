import assert from 'node:assert/strict';
import { test } from 'node:test';
import { find, recording, tick } from './voice-controls.fixtures.ts';

test('native terminal completion preserves the recorded source after SDK duration resets', async () => {
  const f = recording(true);
  await find(f.render(), 'VoiceRecordButton').props.onStart(false);
  f.render();
  f.finishNative();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(dock.props.recording, false);
  assert.equal(dock.props.uri, 'file:///voice/capture.m4a');
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  assert.deepEqual(f.discarded, []);
  assert.deepEqual(f.sent, []);
  await dock.props.onSend(dock.props.session.getSendable());
  assert.deepEqual(f.sent, [4000]);
});

test('media-services reset reviews the original capture, never the replacement SDK file', async () => {
  const f = recording(true);
  await find(f.render(), 'VoiceRecordButton').props.onStart(false);
  f.render();
  f.finishNative(false, true);
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(dock.props.recording, false);
  assert.equal(dock.props.uri, 'file:///voice/capture.m4a');
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  assert.deepEqual(f.discarded, ['file:///voice/reset.m4a']);
  assert.deepEqual(f.sent, []);
  await dock.props.onCancel();
});

test('an interrupted file rejected by native validation remains owned for retry or explicit Cancel', async () => {
  const f = recording(true);
  const prepare = f.voicePort.prepare;
  f.voicePort.prepare = async () => {
    throw new Error('invalid interrupted audio');
  };
  await find(f.render(), 'VoiceRecordButton').props.onStart(false);
  // No duration poll has committed: the terminal path must still validate,
  // rather than deleting this source as an assumed zero-duration recording.
  f.finishNative(true);
  await tick();
  let dock = find(f.render(), 'VoiceDock');
  assert.equal(dock.props.uri, 'file:///voice/capture.m4a');
  assert.equal(dock.props.session.getSendable(), null);
  assert.deepEqual(f.discarded, []);
  assert.deepEqual(f.sent, []);
  f.voicePort.prepare = prepare;
  await dock.props.onRetry();
  await tick();
  dock = find(f.render(), 'VoiceDock');
  assert.ok(dock.props.session.getSendable());
  await dock.props.onCancel();
});

test('foreign and delayed terminal events cannot stop another active or paused capture', async () => {
  const f = recording(true);
  await find(f.render(), 'VoiceRecordButton').props.onStart(false);
  const dock = find(f.render(), 'VoiceDock');
  const status = {
    id: 'other',
    isFinished: true,
    hasError: false,
    error: null,
    url: 'file:///voice/other.m4a',
  };
  f.emitNative(status);
  assert.equal(f.state.isRecording, true);
  await dock.props.onPause();
  f.emitNative({ ...status, id: 'capture' });
  f.emitNative({ ...status, id: 'capture', url: null, hasError: true });
  f.emitNative({ ...status, id: 'capture', url: null, mediaServicesDidReset: true });
  await tick();
  assert.equal(find(f.render(), 'VoiceDock').props.recording, true);
  assert.equal(f.stops(), 0);
  await find(f.render(), 'VoiceDock').props.onCancel();
});
