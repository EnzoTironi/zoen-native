import assert from 'node:assert/strict';
import test from 'node:test';
import {
  attachVoiceTranscript,
  createVoiceDraft,
  editVoiceDraft,
  projectVoiceDraft,
  redoVoiceEdit,
  remapVoiceWaveform,
  undoVoiceEdit,
  voiceRenderPlan,
} from '../media/voice-edit.ts';
import { cut, hasCode, restore, source, toggle, transcript, trim } from './voice-edit.fixtures.ts';

test('a gesture is one undo step; redo survives undo, but a new branch clears redo', () => {
  const initial = createVoiceDraft(source(), transcript);
  const first = editVoiceDraft(initial, [trim(500, 9500), cut(2000, 3000), cut(5000, 6000)]);
  assert.equal(first.undo.length, 1);
  const second = editVoiceDraft(first, toggle('secret'));
  assert.deepEqual(projectVoiceDraft(undoVoiceEdit(second)), projectVoiceDraft(first));
  assert.deepEqual(
    projectVoiceDraft(undoVoiceEdit(undoVoiceEdit(second))),
    projectVoiceDraft(initial),
  );
  assert.deepEqual(
    projectVoiceDraft(redoVoiceEdit(undoVoiceEdit(second))),
    projectVoiceDraft(second),
  );
  const branched = editVoiceDraft(undoVoiceEdit(second), cut(8000, 9000));
  assert.equal(branched.redo.length, 0);
  assert.strictEqual(redoVoiceEdit(branched), branched);
  assert.strictEqual(undoVoiceEdit(initial), initial);
});

test('empty or too-short edits remain undoable and are blocked at render admission', () => {
  const original = createVoiceDraft(source());
  const empty = editVoiceDraft(original, cut(0, 10_000));
  assert.deepEqual(projectVoiceDraft(empty).keptRanges, []);
  assert.equal(projectVoiceDraft(empty).canSend, false);
  assert.throws(() => voiceRenderPlan(empty), hasCode('too_short'));
  assert.deepEqual(projectVoiceDraft(undoVoiceEdit(empty)), projectVoiceDraft(original));
  assert.throws(
    () => voiceRenderPlan(editVoiceDraft(original, trim(0, 249))),
    hasCode('too_short'),
  );
  assert.equal(voiceRenderPlan(editVoiceDraft(original, trim(0, 250))).frameCount, 250);
  const lowRate = createVoiceDraft({ ...source(), sampleRate: 3, frameCount: 3 });
  assert.equal(voiceRenderPlan(editVoiceDraft(lowRate, trim(0, 1))).canSend, true);
});

test('output waveform excludes removed bins and peak-pools retained intervals', () => {
  const initial = createVoiceDraft(source());
  const draft = editVoiceDraft(initial, cut(2000, 4000));
  assert.deepEqual(remapVoiceWaveform(draft, 8), [0.1, 0.2, 0.5, 0.6, 0.7, 0.8, 0.9, 1]);
  assert.deepEqual(remapVoiceWaveform(draft, 4), [0.2, 0.6, 0.8, 1]);
  assert.deepEqual(remapVoiceWaveform(editVoiceDraft(initial, cut(1000, 9000)), 2), [0.1, 1]);
  assert.deepEqual(remapVoiceWaveform(editVoiceDraft(initial, cut(0, 10_000))), []);
  assert.deepEqual(remapVoiceWaveform(createVoiceDraft({ ...source(), levels: [] })), []);
  for (const bars of [0, 161, NaN, 1.5])
    assert.throws(() => remapVoiceWaveform(draft, bars), hasCode('invalid_waveform'));
});

test('late transcript binds source identity, preserves all cuts and rejects invalid timing', () => {
  const initial = editVoiceDraft(createVoiceDraft(source()), cut(1500, 1750));
  assert.throws(
    () => attachVoiceTranscript(initial, 'another-recording', transcript),
    hasCode('source_changed'),
  );
  const attached = attachVoiceTranscript(initial, initial.original.id, transcript);
  assert.strictEqual(attached.original, initial.original);
  assert.strictEqual(attached.edit, initial.edit);
  assert.equal(projectVoiceDraft(attached).transcript, 'Keep, Bye!');
  assert.throws(
    () => attachVoiceTranscript(attached, initial.original.id, transcript),
    hasCode('transcript_already_attached'),
  );
  for (const words of [
    [{ ...transcript.words[0]!, startFrame: -1 }],
    [{ ...transcript.words[0]!, endFrame: Infinity }],
    [{ ...transcript.words[0]!, endFrame: 0 }],
    [{ ...transcript.words[0]!, endFrame: 10_001 }],
    [transcript.words[0]!, { ...transcript.words[1]!, startFrame: 999 }],
    [transcript.words[0]!, { ...transcript.words[1]!, id: 'keep' }],
  ])
    assert.throws(
      () => attachVoiceTranscript(initial, initial.original.id, { text: 'bad', words }),
      hasCode('invalid_transcript'),
    );
  assert.equal(initial.transcript, null);
  assert.equal(voiceRenderPlan(initial).frameCount, 9750);
});

test('independent per-frame oracle verifies 100 deterministic cut/trim combinations', () => {
  let random = 0x58;
  const next = () => {
    random = (Math.imul(random, 1664525) + 1013904223) >>> 0;
    return random % 1001;
  };
  for (let fixture = 0; fixture < 100; fixture++) {
    const a = next(),
      b = next(),
      start = Math.min(a, b),
      end = Math.max(a, b);
    const mask = Array.from({ length: 1000 }, (_, frame) => frame >= start && frame < end);
    let draft = editVoiceDraft(
      createVoiceDraft({ ...source(), frameCount: 1000 }),
      trim(start, end),
    );
    for (let index = 0; index < 7; index++) {
      const a = next(),
        b = next(),
        low = Math.min(a, b),
        high = Math.max(a, b);
      draft = editVoiceDraft(draft, cut(low, high));
      for (let frame = low; frame < high; frame++) mask[frame] = false;
    }
    const output = projectVoiceDraft(draft);
    const reconstructed = Array.from({ length: 1000 }, () => 0);
    for (const range of output.keptRanges)
      for (let frame = range.startFrame; frame < range.endFrame; frame++) reconstructed[frame]!++;
    assert.deepEqual(reconstructed, mask.map(Number));
    assert.equal(output.frameCount, mask.filter(Boolean).length);
    assert.ok(
      output.keptRanges.every(
        (range, index) => index === 0 || output.keptRanges[index - 1]!.endFrame < range.startFrame,
      ),
    );
    const restoreA = next(),
      restoreB = next();
    const restoreStart = Math.min(restoreA, restoreB),
      restoreEnd = Math.max(restoreA, restoreB);
    draft = editVoiceDraft(draft, restore(restoreStart, restoreEnd));
    for (let frame = restoreStart; frame < restoreEnd; frame++) mask[frame] = true;
    const restored = projectVoiceDraft(draft);
    const restoredMask = Array.from({ length: 1000 }, () => false);
    for (const range of restored.keptRanges)
      for (let frame = range.startFrame; frame < range.endFrame; frame++)
        restoredMask[frame] = true;
    assert.deepEqual(restoredMask, mask);
    assert.equal(restored.frameCount, mask.filter(Boolean).length);
  }
});
