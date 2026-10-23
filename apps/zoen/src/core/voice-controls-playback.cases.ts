import assert from 'node:assert/strict';
import { test } from 'node:test';
import { VoiceSession } from '../media/voice-session.ts';
import {
  component,
  find,
  recording,
  stagedPreview,
  tick,
  voicePort,
} from './voice-controls.fixtures.ts';

test('ordinary StrictMode effect replay allows a fresh capture and current shared preview', async () => {
  const f = recording(true);
  f.render();
  f.app.replayEffects();
  const { preview, session } = await stagedPreview(f);
  assert.equal(f.records(), 1);
  assert.equal(preview.props.isCurrent(session), true);
  assert.equal(preview.props.isCurrent(new VoiceSession(voicePort())), false);
  await find(f.render(), 'VoiceDock').props.onCancel();
});

test('actual parent preview gate observes focus, AppState and suspension without stale rendered admission', async () => {
  const f = recording(true);
  const { preview, session } = await stagedPreview(f);
  const current = preview.props.isCurrent;
  assert.equal(current(session), true);
  f.app.blur();
  assert.equal(current(session), false, 'blur revokes before another React render');
  f.app.focus();
  assert.equal(current(session), true);
  f.background();
  assert.equal(current(session), false, 'background revokes before another React render');
  f.background('active');
  assert.equal(current(session), true);
  for (const flag of ['suspended', 'disabled']) {
    const blocked = find(f.render({ [flag]: true }), 'VoicePreviewPlayback');
    assert.strictEqual(blocked.props.session, session);
    assert.equal(current(session), false);
    assert.equal(blocked.props.disabled, true);
    f.render({ [flag]: false });
    assert.equal(current(session), true);
  }
  await find(f.render(), 'VoiceDock').props.onCancel();
});

test('device cleanup permanently retires parent playback before await and survives rerender/effect replay', async () => {
  const f = recording(true);
  const { preview, session } = await stagedPreview(f);
  assert.equal(preview.props.isCurrent(session), true);
  const cleanup = [...f.cleanups][0]!;
  const draining = cleanup();
  assert.equal(preview.props.isCurrent(session), false, 'retirement is synchronous');
  const next = find(f.render(), 'VoicePreviewPlayback');
  assert.strictEqual(next.props.session, session);
  assert.equal(next.props.isCurrent(session), false);
  f.app.replayEffects();
  f.app.focus();
  const replayed = find(f.render(), 'VoicePreviewPlayback');
  assert.equal(
    replayed.props.isCurrent(session),
    false,
    'effect setup cannot undo device retirement',
  );
  await draining;
  await tick();
  assert.equal(find(f.render(), 'VoicePreviewPlayback').props.isCurrent(session), false);
  f.app.unmount();
  await tick();
});

test('expanding and collapsing the actual composer and dock do not pause the shared preview', async () => {
  const f = recording(true);
  const { tree, preview, session } = await stagedPreview(f);
  const before = f.events.filter((event) => event === 'playback:pause').length;
  const dock = component('voice-dock', {
    './AudioClip': {
      AudioClip: 'AudioClip',
      AudioClipControls: 'AudioClipControls',
      clipTime: String,
      pauseAudioPlayback: () => f.events.push('playback:pause'),
    },
  });
  const compact = dock.render(find(tree, 'VoiceDock').props);
  find(compact, 'IconButton', 'media.voiceExpand').props.onPress();
  const expanded = f.render();
  assert.equal(find(expanded, 'MediaSheet').props.visible, true);
  assert.strictEqual(find(expanded, 'VoicePreviewPlayback').props.session, session);
  assert.equal(preview.props.isCurrent(session), true);
  find(expanded, 'IconButton', 'media.voiceCollapse').props.onPress();
  const collapsed = f.render();
  assert.equal(find(collapsed, 'MediaSheet').props.visible, false);
  assert.ok(find(collapsed, 'VoiceDock'));
  assert.strictEqual(find(collapsed, 'VoicePreviewPlayback').props.session, session);
  assert.equal(f.events.filter((event) => event === 'playback:pause').length, before);
  await find(collapsed, 'VoiceDock').props.onCancel();
});

for (const action of ['Send', 'Cancel'] as const)
  test(`${action} immediately pauses and blocks the actual parent preview before React commits`, async () => {
    const f = recording(true);
    const { tree, preview, session } = await stagedPreview(f);
    const before = f.events.filter((event) => event === 'playback:pause').length;
    const props = find(tree, 'VoiceDock').props;
    const pending = action === 'Send' ? props.onSend(session.getSendable()) : props.onCancel();
    assert.equal(f.events.filter((event) => event === 'playback:pause').length, before + 1);
    assert.equal(
      preview.props.isCurrent(session),
      false,
      'busy admission closes before stop awaits',
    );
    await pending;
    assert.equal(find(f.render(), 'VoicePreviewPlayback').props.session, null);
    assert.deepEqual(f.sent, action === 'Send' ? [4000] : []);
    f.app.unmount();
    await tick();
  });

test('audio mode restore failure does not poison stop or prevent retrying discard', async () => {
  const f = recording();
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  assert.equal(f.state.isRecording, true);
  f.failRestore(true);
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /audio mode restore failure/);
  assert.equal(f.state.isRecording, false);
  assert.equal(f.discarded.length, 0);
  // A second failed restoration must still retain ownership, rather than claiming discard.
  await find(f.render(), 'VoiceDock').props.onCancel();
  assert.equal(f.discarded.length, 0);
  f.failRestore(false);
  await find(f.render(), 'VoiceDock').props.onCancel();
  assert.deepEqual(f.discarded, ['file:///voice/capture.m4a']);
  assert.equal(f.stops(), 1);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  await find(f.render(), 'VoiceDock').props.onStop();
  assert.equal(f.stops(), 2);
});

test('failed recorder stop keeps the live recording owned until a confirmed stop retry', async () => {
  const f = recording();
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.failStop(true);
  await find(f.render(), 'VoiceDock').props.onCancel();
  assert.equal(f.state.isRecording, true);
  assert.equal(f.discarded.length, 0);
  f.failStop(false);
  await find(f.render(), 'VoiceDock').props.onCancel();
  assert.equal(f.state.isRecording, false);
  assert.deepEqual(f.discarded, ['file:///voice/capture.m4a']);
  assert.equal(f.stops(), 2);
});

test('retry reconciles a completed native stop after its confirmation read failed', async () => {
  const f = recording();
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.failConfirmation();
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /confirmation failure/);
  assert.equal(f.state.isRecording, false);
  assert.equal(f.discarded.length, 0);
  await find(f.render(), 'VoiceDock').props.onCancel();
  await tick();
  assert.equal(f.stops(), 1, 'a stopped recorder must not receive a second native stop');
  assert.deepEqual(f.discarded, ['file:///voice/capture.m4a']);
});

test('a retained cleanup after unmount never calls the SDK-released recorder', async () => {
  const f = recording();
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  const cleanup = [...f.cleanups][0]!;
  f.releaseRecorder();
  f.app.unmount();
  await cleanup();
  assert.equal(f.releasedCalls(), 0);
  assert.ok(f.discarded.includes('file:///voice/capture.m4a'));
});

test('the actual dock exposes non-destructive audio-mode recovery beside a playable preview', async () => {
  const f = recording(true);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  f.failRestore(true);
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /restore failure/);
  await tick();
  const props = find(f.render(), 'VoiceDock').props;
  const session = props.session;
  const captured = session.getSendable();
  const dock = component('voice-dock', {
    './voice-preview-playback': {
      useVoicePreviewPlayback: () => ({ session, preview: captured.preview, playback: {} }),
    },
  });
  const tree = dock.render(props);
  assert.ok(find(tree, 'AudioClipControls'));
  f.failRestore(false);
  find(tree, 'IconButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  const recovered = find(f.render(), 'VoiceDock').props;
  assert.equal(recovered.failure, null);
  assert.strictEqual(recovered.session, session);
  assert.strictEqual(session.getSendable().preview, captured.preview);
  assert.deepEqual(f.discarded, []);
  await recovered.onSend(session.getSendable());
  assert.deepEqual(f.sent, [4000]);
  await tick();
});
