import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as voiceEdit from '../media/voice-edit.ts';
import { auditionLabels, auditionWave, find, nodes } from './voice-controls.fixtures.ts';

test('retained waveform taps seek the edited player without edits; removed taps still restore', () => {
  const draft = voiceEdit.editVoiceDraft(
    voiceEdit.createVoiceDraft({
      id: 'audition-original',
      sampleRate: 1000,
      frameCount: 10_000,
      levels: [0.2, 0.8],
    }),
    { type: 'cut', range: { startFrame: 2000, endFrame: 3000 } },
  );
  const edits: unknown[] = [];
  const seeks: [voiceEdit.VoiceDraft, number][] = [];
  const props = {
    draft,
    onEdit: (command: unknown) => edits.push(command),
    onTrimPreviewChange() {},
    onSeekOriginal: (observed: voiceEdit.VoiceDraft, frame: number) =>
      seeks.push([observed, frame]),
    auditionLabels,
  };
  const { tree } = auditionWave(props);
  const tap = find(tree, 'GestureDetector').props.gesture[1];
  tap.onBegin();
  tap.onDeactivate({ canceled: false, x: 22 + (3200 / 10_000) * 200 });
  assert.equal(seeks.length, 1);
  assert.strictEqual(seeks[0]![0], draft);
  assert.equal(seeks[0]![1], 3200);
  assert.deepEqual(edits, []);
  assert.equal(draft.undo.length, 1);
  tap.onBegin();
  tap.onDeactivate({ canceled: false, x: 22 + (2500 / 10_000) * 200 });
  assert.deepEqual(JSON.parse(JSON.stringify(edits)), [
    { type: 'restore', range: { startFrame: 2000, endFrame: 3000 } },
  ]);
  assert.equal(seeks.length, 1);
  tap.onBegin();
  tap.onDeactivate({ canceled: true, x: 100 });
  assert.equal(seeks.length, 1, 'a canceled/vertical gesture cannot become an audition');
});

test('pending trim blocks stale retained tap, selection play and accessibility seek until Discard', () => {
  const draft = voiceEdit.createVoiceDraft({
    id: 'pending-audition',
    sampleRate: 1000,
    frameCount: 10_000,
    levels: [0.4],
  });
  const seeks: number[] = [];
  const plays: voiceEdit.FrameRange[] = [];
  const edits: unknown[] = [];
  const props = {
    draft,
    onEdit: (command: unknown) => edits.push(command),
    onTrimPreviewChange() {},
    onSeekOriginal: (_observed: voiceEdit.VoiceDraft, frame: number) => seeks.push(frame),
    onPlayFromSelection: (_observed: voiceEdit.VoiceDraft, range: voiceEdit.FrameRange) =>
      plays.push(range),
    playheadOriginalFrame: 1000,
    auditionLabels,
  };
  const { waveform, tree } = auditionWave(props);
  nodes(tree)
    .filter((node) => node.type === 'Slider')[2]!
    .props.onValueChange(0.1);
  const selected = waveform.render(props);
  const stalePlay = find(selected, 'Button', auditionLabels.playFromSelection);
  const staleTrack = nodes(selected).find((node) => node.props.accessibilityRole === 'adjustable')!;
  const staleTap = find(selected, 'GestureDetector').props.gesture[1];
  staleTap.onBegin();
  find(selected, 'Slider').props.onValueChange(0.2);
  staleTap.onDeactivate({ canceled: false, x: 100 });
  stalePlay.props.onPress();
  staleTrack.props.onAccessibilityAction({ nativeEvent: { actionName: 'increment' } });
  assert.deepEqual(seeks, []);
  assert.deepEqual(plays, []);
  assert.deepEqual(edits, []);
  const pending = waveform.render(props);
  assert.ok(
    find(pending, 'Button', auditionLabels.playFromSelection).props.modifiers.some(
      ([name, value]: [string, boolean]) => name === 'disabled' && value,
    ),
  );
  assert.equal(
    nodes(pending).some((node) => node.props.testID === 'voice-editor-playhead'),
    false,
  );
  find(pending, 'Button', 'media.voiceDiscardTrim').props.onPress();
  stalePlay.props.onPress();
  assert.deepEqual(JSON.parse(JSON.stringify(plays)), [{ startFrame: 1000, endFrame: 10_000 }]);
  assert.deepEqual(edits, []);
});

test('selection audition uses the latest native endpoints and refuses a fully removed selection', () => {
  const draft = voiceEdit.editVoiceDraft(
    voiceEdit.createVoiceDraft({
      id: 'selection-audition',
      sampleRate: 1000,
      frameCount: 10_000,
      levels: [0.4],
    }),
    { type: 'cut', range: { startFrame: 2000, endFrame: 3000 } },
  );
  const plays: voiceEdit.FrameRange[] = [];
  const props = {
    draft,
    onEdit() {},
    onTrimPreviewChange() {},
    onSeekOriginal() {},
    onPlayFromSelection: (_observed: voiceEdit.VoiceDraft, range: voiceEdit.FrameRange) =>
      plays.push(range),
    auditionLabels,
  };
  const { waveform, tree } = auditionWave(props);
  nodes(tree)
    .filter((node) => node.type === 'Slider')[2]!
    .props.onValueChange(0.1);
  const selected = waveform.render(props);
  const stalePlay = find(selected, 'Button', auditionLabels.playFromSelection);
  const sliders = nodes(selected).filter((node) => node.type === 'Slider');
  sliders[2]!.props.onValueChange(0.2);
  sliders[3]!.props.onValueChange(7 / 9);
  stalePlay.props.onPress();
  assert.deepEqual(JSON.parse(JSON.stringify(plays)), [{ startFrame: 2000, endFrame: 8000 }]);
  const current = waveform.render(props);
  const end = nodes(current).filter((node) => node.type === 'Slider')[3]!;
  end.props.onValueChange(1 / 8);
  // The current slider domain is [2000,10000], so this selects exactly [2000,3000).
  stalePlay.props.onPress();
  assert.equal(plays.length, 1);
  const removed = waveform.render(props);
  assert.ok(
    find(removed, 'Button', auditionLabels.playFromSelection).props.modifiers.some(
      ([name, value]: [string, boolean]) => name === 'disabled' && value,
    ),
  );
});

test('original waveform accessibility seeks edited time across cuts and displays the mapped playhead', () => {
  const draft = voiceEdit.editVoiceDraft(
    voiceEdit.createVoiceDraft({
      id: 'accessible-audition',
      sampleRate: 1000,
      frameCount: 10_000,
      levels: [0.3],
    }),
    { type: 'cut', range: { startFrame: 2000, endFrame: 3000 } },
  );
  const seeks: number[] = [];
  const props = {
    draft,
    onEdit() {},
    onTrimPreviewChange() {},
    onSeekOriginal: (observed: voiceEdit.VoiceDraft, frame: number) => {
      assert.strictEqual(observed, draft);
      seeks.push(frame);
    },
    playheadOriginalFrame: 1500,
    auditionLabels,
  };
  const { waveform, tree } = auditionWave(props);
  const track = nodes(tree).find((node) => node.props.accessibilityRole === 'adjustable')!;
  track.props.onAccessibilityAction({ nativeEvent: { actionName: 'increment' } });
  assert.deepEqual(seeks, [7500]);
  assert.equal(
    nodes(tree).find((node) => node.props.testID === 'voice-editor-playhead')!.props.style.left,
    51,
  );
  const locked = waveform.render({ ...props, auditionDisabled: true });
  const lockedTrack = nodes(locked).find((node) => node.props.accessibilityRole === 'adjustable')!;
  assert.equal(lockedTrack.props.accessibilityState.disabled, true);
  lockedTrack.props.onAccessibilityAction({ nativeEvent: { actionName: 'decrement' } });
  assert.deepEqual(seeks, [7500]);
});
