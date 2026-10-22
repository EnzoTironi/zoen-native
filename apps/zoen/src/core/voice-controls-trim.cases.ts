import assert from 'node:assert/strict';
import { test } from 'node:test';
import { VoiceSession } from '../media/voice-session.ts';
import { component, find, nodes, recording, tick, voicePort } from './voice-controls.fixtures.ts';

test('precision slider blocks an already-rendered Send handler before React commits', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const sent: number[] = [];
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = {
    session,
    initiallyEditing: true,
    onSend: async (value: { preview: { durationMs: number } }) =>
      sent.push(value.preview.durationMs),
  };
  const tree = review.render(props);
  const staleSend = find(tree, 'NativeButton', 'media.send');
  const staleDone = find(tree, 'NativeButton', 'media.voiceDoneEditing');
  const waveProps = find(tree, 'VoiceWaveform').props;
  const wave = waveform.render(waveProps);
  find(wave, 'Slider').props.onValueChange(0.25);
  staleDone.props.onPress();
  staleSend.props.onPress();
  await tick();
  assert.deepEqual(sent, [], 'the canonical old audio must not escape via a stale rendered button');
  assert.equal(find(review.render(props), 'NativeButton', 'media.send').props.disabled, true);
  assert.ok(find(review.render(props), 'VoiceWaveform'), 'stale Done must not hide pending edits');
  assert.equal(
    nodes(review.render(props)).some((node) => node.type === 'AudioClip'),
    false,
  );
  const changed = waveform.render(waveProps);
  find(changed, 'Button', 'media.voiceDiscardTrim').props.onPress();
  find(review.render(props), 'NativeButton', 'media.send').props.onPress();
  await tick();
  assert.deepEqual(sent, [4000]);
  await session.close();
});

test('unapplied precision blocks stale word, suggestion, selection and history actions before React commits', async () => {
  const native = voicePort();
  native.transcribe = async () => ({
    status: 'ready',
    text: 'Keep um after',
    words: [
      { id: 'keep', text: 'Keep', startFrame: 0, endFrame: 500 },
      { id: 'um', text: 'um', startFrame: 500, endFrame: 800 },
      { id: 'after', text: 'after', startFrame: 2500, endFrame: 3500 },
    ],
  });
  const session = new VoiceSession(native);
  await session.prepare('file:///voice/capture.m4a');
  await session.transcribe('en');
  session.edit({ type: 'trim', range: { startFrame: 0, endFrame: 3500 } });
  session.edit({ type: 'cut', range: { startFrame: 1500, endFrame: 1600 } });
  session.undo();
  await session.retryPreview();
  const applied = session.getSnapshot();
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = { session, initiallyEditing: true, onSend: async () => {} };
  const tree = review.render(props);
  const waveProps = find(tree, 'VoiceWaveform').props;
  const initialWave = waveform.render(waveProps);
  nodes(initialWave)
    .filter((node) => node.type === 'Slider')[2]!
    .props.onValueChange(0.24);
  const selectedWave = waveform.render(waveProps);
  const staleSelection = find(selectedWave, 'Button', 'media.voiceCutSelection');
  const staleUndo = find(tree, 'Button', 'media.voiceUndo');
  const staleRedo = find(tree, 'Button', 'media.voiceRedo');
  const staleFillers = find(tree, 'NativeButton', 'media.voiceRemoveFillers');
  const stalePauses = find(tree, 'NativeButton', 'media.voiceShortenPauses');
  const staleWords = find(tree, 'VoiceWords');
  find(selectedWave, 'Slider').props.onValueChange(0.1);
  const pending = session.getSnapshot().pendingTrim;
  assert.ok(pending);

  staleUndo.props.onPress();
  staleRedo.props.onPress();
  staleWords.props.onEdit({ type: 'setWordRemoved', wordId: 'um', removed: true });
  staleFillers.props.onPress();
  stalePauses.props.onPress();
  staleSelection.props.onPress();
  assert.strictEqual(session.getSnapshot().draft, applied.draft);
  assert.strictEqual(session.getSnapshot().source, applied.source);
  assert.deepEqual(session.getSnapshot().pendingTrim, pending);
  assert.equal(session.getSnapshot().error, null);

  const current = review.render(props);
  assert.equal(find(current, 'VoiceWords').props.disabled, true);
  assert.equal(find(current, 'NativeButton', 'media.voiceRemoveFillers').props.disabled, true);
  assert.equal(find(current, 'NativeButton', 'media.voiceShortenPauses').props.disabled, true);
  for (const key of ['media.voiceUndo', 'media.voiceRedo'])
    assert.ok(
      find(current, 'Button', key).props.modifiers.some(
        ([name, disabled]: [string, boolean]) => name === 'disabled' && disabled,
      ),
    );
  const pendingWave = waveform.render(waveProps);
  assert.ok(
    find(pendingWave, 'Button', 'media.voiceCutSelection').props.modifiers.some(
      ([name, disabled]: [string, boolean]) => name === 'disabled' && disabled,
    ),
  );
  find(pendingWave, 'Button', 'media.voiceDiscardTrim').props.onPress();
  assert.equal(session.getSnapshot().pendingTrim, null);
  staleSelection.props.onPress();
  assert.notStrictEqual(session.getSnapshot().draft, applied.draft);
  assert.equal(session.getSnapshot().draft!.edit.cuts.length, 1);
  await session.close();
});

test('pending trim blocks parent send and stale sheet collapse until explicit discard', async () => {
  const f = recording(true);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  await find(f.render(), 'VoiceDock').props.onStop();
  await tick();
  find(f.render(), 'VoiceDock').props.onExpand();
  const parent = f.render();
  const sheet = find(parent, 'MediaSheet');
  const collapse = find(parent, 'IconButton', 'media.voiceCollapse');
  const reviewProps = find(parent, 'VoiceReview').props;
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const waveProps = find(review.render(reviewProps), 'VoiceWaveform').props;
  find(waveform.render(waveProps), 'Slider').props.onValueChange(0.25);
  collapse.props.onPress();
  sheet.props.onDismiss();
  assert.equal(find(f.render(), 'MediaSheet').props.visible, true);
  assert.equal(find(f.render(), 'MediaSheet').props.locked, true);
  await assert.rejects(reviewProps.onSend(reviewProps.session.getSendable()));
  assert.deepEqual(f.sent, []);
  find(waveform.render(waveProps), 'Button', 'media.voiceDiscardTrim').props.onPress();
  assert.equal(find(f.render(), 'MediaSheet').props.locked, false);
  await reviewProps.onSend(reviewProps.session.getSendable());
  assert.deepEqual(f.sent, [4000]);
  await tick();
});

test('canonical draft changes invalidate old previews without letting delayed callbacks unlock new ones', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const sent: number[] = [];
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = {
    session,
    initiallyEditing: true,
    onSend: async (value: { preview: { durationMs: number } }) =>
      sent.push(value.preview.durationMs),
  };
  const old = find(review.render(props), 'VoiceWaveform').props;
  find(waveform.render(old), 'Slider').props.onValueChange(0.25);
  find(waveform.render(old), 'Button', 'media.voiceDiscardTrim').props.onPress();
  session.edit({ type: 'trim', range: { startFrame: 0, endFrame: 3500 } });
  await tick();
  const current = find(review.render(props), 'VoiceWaveform').props;
  assert.equal(find(review.render(props), 'NativeButton', 'media.send').props.disabled, false);
  find(waveform.render(current), 'Slider').props.onValueChange(0.2);
  old.onTrimPreviewChange(old.draft, false);
  old.onTrimPreviewChange(old.draft, true);
  old.onEdit({ type: 'trim', range: { startFrame: 0, endFrame: 1000 } });
  assert.strictEqual(session.getSnapshot().draft, current.draft);
  assert.equal(find(review.render(props), 'NativeButton', 'media.send').props.disabled, true);
  find(waveform.render(current), 'Button', 'media.voiceDiscardTrim').props.onPress();
  find(review.render(props), 'NativeButton', 'media.send').props.onPress();
  await tick();
  assert.deepEqual(sent, [3500]);
  await session.close();
});

test('cleanup of an old review cannot unlock a newer review of the same canonical draft', async () => {
  const f = recording(true);
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  await find(f.render(), 'VoiceDock').props.onStop();
  await tick();
  find(f.render(), 'VoiceDock').props.onExpand();
  const props = find(f.render(), 'VoiceReview').props;
  const old = component('voice-review.ios', {});
  const current = component('voice-review.ios', {});
  const oldWave = component('voice-waveform.ios', {});
  const currentWave = component('voice-waveform.ios', {});
  const oldProps = find(old.render(props), 'VoiceWaveform').props;
  const currentProps = find(current.render(props), 'VoiceWaveform').props;
  find(oldWave.render(oldProps), 'Slider').props.onValueChange(0.25);
  find(currentWave.render(currentProps), 'Slider').props.onValueChange(0.5);
  old.unmount();
  assert.equal(find(f.render(), 'MediaSheet').props.locked, true);
  await assert.rejects(props.onSend(props.session.getSendable()));
  assert.deepEqual(f.sent, []);
  find(currentWave.render(currentProps), 'Button', 'media.voiceDiscardTrim').props.onPress();
  assert.equal(find(f.render(), 'MediaSheet').props.locked, false);
  await props.onSend(props.session.getSendable());
  assert.deepEqual(f.sent, [4000]);
  await tick();
});

test('an old source or unmounted review cannot send or unlock the current source preview', async () => {
  const oldSession = new VoiceSession(voicePort());
  const session = new VoiceSession(voicePort());
  await Promise.all([oldSession.prepare('file:///old.m4a'), session.prepare('file:///new.m4a')]);
  const sent: string[] = [];
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const oldTree = review.render({
    session: oldSession,
    initiallyEditing: true,
    onSend: async () => sent.push('old'),
  });
  const oldSend = find(oldTree, 'NativeButton', 'media.send');
  const oldWave = find(oldTree, 'VoiceWaveform').props;
  const props = { session, initiallyEditing: true, onSend: async () => sent.push('current') };
  // A session change closes the old editor; reopen the current source explicitly.
  find(review.render(props), 'NativeButton', 'media.voiceEdit').props.onPress();
  const tree = review.render(props);
  const wave = find(tree, 'VoiceWaveform').props;
  find(waveform.render(wave), 'Slider').props.onValueChange(0.25);
  oldWave.onTrimPreviewChange(oldWave.draft, false);
  oldSend.props.onPress();
  await tick();
  assert.deepEqual(sent, []);
  assert.equal(find(review.render(props), 'NativeButton', 'media.send').props.disabled, true);
  const currentSend = find(review.render(props), 'NativeButton', 'media.send');
  review.unmount();
  currentSend.props.onPress();
  await tick();
  assert.deepEqual(sent, []);
  await Promise.all([oldSession.close(), session.close()]);
});

test('Apply commits the exact preview before Send can capture it', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const sent: number[] = [];
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = {
    session,
    initiallyEditing: true,
    onSend: async (value: { preview: { durationMs: number } }) =>
      sent.push(value.preview.durationMs),
  };
  const waveProps = find(review.render(props), 'VoiceWaveform').props;
  find(waveform.render(waveProps), 'Slider').props.onValueChange(0.25);
  find(waveform.render(waveProps), 'Button', 'media.voiceApplyTrim').props.onPress();
  await tick();
  find(review.render(props), 'NativeButton', 'media.send').props.onPress();
  await tick();
  assert.deepEqual(sent, [3000]);
  await session.close();
});

test('a stale Apply handler commits the latest native slider value before React renders', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = { session, initiallyEditing: true, onSend: async () => {} };
  const waveProps = find(review.render(props), 'VoiceWaveform').props;
  find(waveform.render(waveProps), 'Slider').props.onValueChange(0.25);
  const previous = waveform.render(waveProps);
  const staleApply = find(previous, 'Button', 'media.voiceApplyTrim');
  find(previous, 'Slider').props.onValueChange(0.5);
  staleApply.props.onPress();
  await tick();
  assert.equal(session.getSendable()?.preview.durationMs, 2000);
  await session.close();
});

test('start and end slider events in one render preserve both latest endpoints', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = { session, initiallyEditing: true, onSend: async () => {} };
  const waveProps = find(review.render(props), 'VoiceWaveform').props;
  const original = waveform.render(waveProps);
  const sliders = nodes(original).filter((node) => node.type === 'Slider');
  sliders[0]!.props.onValueChange(0.25);
  sliders[1]!.props.onValueChange(0.75);
  find(waveform.render(waveProps), 'Button', 'media.voiceApplyTrim').props.onPress();
  await tick();
  assert.deepEqual(session.getSnapshot().draft?.edit.trim, { startFrame: 1000, endFrame: 3000 });
  assert.equal(session.getSendable()?.preview.durationMs, 2000);
  await session.close();
});
