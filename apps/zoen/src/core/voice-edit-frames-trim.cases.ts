import assert from 'node:assert/strict';
import test from 'node:test';
import {
  createVoiceDraft,
  editVoiceDraft,
  editedToOriginalFrame,
  originalToEditedFrame,
  projectVoiceDraft,
  voiceSelectionToEditedRange,
} from '../media/voice-edit.ts';
import { cut, hasCode, setWordRemoved, source, transcript, trim } from './voice-edit.fixtures.ts';

test('audition maps retained frames and selections through trim, cuts and a removed word', () => {
  const draft = editVoiceDraft(
    createVoiceDraft(source(), {
      text: 'removed',
      words: [{ id: 'removed', text: 'removed', startFrame: 7000, endFrame: 7500 }],
    }),
    [trim(1000, 9000), cut(2000, 3000), cut(5000, 6000), setWordRemoved('removed', true)],
  );
  const projection = projectVoiceDraft(draft);
  assert.equal(projection.frameCount, 5500);
  assert.equal(originalToEditedFrame(projection, 3200), 1200);
  assert.equal(originalToEditedFrame(projection, 2500), 1000);
  assert.equal(originalToEditedFrame(projection, -10), 0);
  assert.equal(originalToEditedFrame(projection, 20_000), 5500);
  for (const [edited, original] of [
    [0, 1000],
    [999, 1999],
    [1000, 3000],
    [3000, 6000],
    [4000, 7500],
    [5500, 9000],
    [20_000, 9000],
    [-1, 1000],
    [1000.5, 3000.5],
  ])
    assert.equal(editedToOriginalFrame(projection, edited!), original);
  assert.deepEqual(voiceSelectionToEditedRange(projection, { startFrame: 1800, endFrame: 6200 }), {
    startFrame: 800,
    endFrame: 3200,
  });
  assert.deepEqual(voiceSelectionToEditedRange(projection, { startFrame: 2100, endFrame: 3500 }), {
    startFrame: 1000,
    endFrame: 1500,
  });
  assert.equal(voiceSelectionToEditedRange(projection, { startFrame: 2100, endFrame: 2900 }), null);
  assert.equal(voiceSelectionToEditedRange(projection, { startFrame: 2000, endFrame: 3000 }), null);
});

test('audition round trips never land inside removed audio and follow a restored fragment', () => {
  const initial = createVoiceDraft(source(), transcript);
  const cutDraft = editVoiceDraft(initial, [trim(500, 8000), cut(1200, 1800)]);
  const restored = editVoiceDraft(cutDraft, setWordRemoved('secret', false));
  for (const draft of [cutDraft, restored, editVoiceDraft(restored, trim(0, 10_000))]) {
    const projection = projectVoiceDraft(draft);
    for (let output = 0; output < projection.frameCount; output += 13) {
      const original = editedToOriginalFrame(projection, output)!;
      assert.ok(
        projection.keptRanges.some(
          (kept) => kept.startFrame <= original && original < kept.endFrame,
        ),
      );
      assert.equal(originalToEditedFrame(projection, original), output);
    }
  }
  assert.equal(editedToOriginalFrame(projectVoiceDraft(cutDraft), 700), 1800);
  assert.equal(editedToOriginalFrame(projectVoiceDraft(restored), 700), 1200);
});

test('empty edits and nonfinite audition inputs have no seekable or playable position', () => {
  const full = projectVoiceDraft(createVoiceDraft(source()));
  const empty = projectVoiceDraft(editVoiceDraft(createVoiceDraft(source()), cut(0, 10_000)));
  for (const projection of [full, empty])
    for (const value of [NaN, Infinity, -Infinity]) {
      assert.equal(originalToEditedFrame(projection, value), null);
      assert.equal(editedToOriginalFrame(projection, value), null);
      assert.equal(
        voiceSelectionToEditedRange(projection, { startFrame: value, endFrame: 5000 }),
        null,
      );
    }
  assert.equal(originalToEditedFrame(empty, 0), null);
  assert.equal(editedToOriginalFrame(empty, 0), null);
  assert.equal(voiceSelectionToEditedRange(empty, { startFrame: 0, endFrame: 5000 }), null);
  for (const selection of [
    { startFrame: 5000, endFrame: 5000 },
    { startFrame: 6000, endFrame: 5000 },
    { startFrame: 11_000, endFrame: 12_000 },
  ])
    assert.equal(voiceSelectionToEditedRange(full, selection), null);
});

test('audio descriptor, transcript and edit history are defensive immutable copies', () => {
  const input = { ...source(), levels: [0.2, 0.4] };
  const text = { text: transcript.text, words: transcript.words.map((word) => ({ ...word })) };
  const draft = createVoiceDraft(input, text);
  input.levels[0] = 1;
  text.words[0]!.text = 'changed';
  const edited = editVoiceDraft(draft, [trim(500, 9500), cut(2000, 3000)]);
  assert.equal(draft.original.levels[0], 0.2);
  assert.equal(draft.transcript!.words[0]!.text, 'Keep,');
  assert.equal(projectVoiceDraft(draft).frameCount, 10_000);
  assert.equal(projectVoiceDraft(edited).frameCount, 8000);
  assert.strictEqual(edited.original, draft.original);
  assert.ok(Object.isFrozen(draft.original) && Object.isFrozen(draft.original.levels));
  assert.ok(Object.isFrozen(edited.edit.cuts) && Object.isFrozen(edited.undo));
  assert.throws(() => Object.assign(draft.original, { frameCount: 1 }), TypeError);
});

test('source is bounded to one hour and exact integer frames, with finite normalized waveform', () => {
  for (const invalid of [
    { sampleRate: 0 },
    { sampleRate: 1.5 },
    { sampleRate: NaN },
    { sampleRate: Number.MAX_SAFE_INTEGER },
    { frameCount: 0 },
    { frameCount: 2.1 },
    { frameCount: Infinity },
    { frameCount: 3_600_001 },
    { id: '' },
  ])
    assert.throws(() => createVoiceDraft({ ...source(), ...invalid }), hasCode('invalid_source'));
  assert.equal(
    createVoiceDraft({ ...source(), frameCount: 3_600_000 }).original.frameCount,
    3_600_000,
  );
  for (const levels of [[NaN], [Infinity], [-0.1], [1.01]])
    assert.throws(() => createVoiceDraft({ ...source(), levels }), hasCode('invalid_waveform'));
});

test('overlapping and adjacent cuts normalize, clamp to source and remain half-open', () => {
  const draft = editVoiceDraft(createVoiceDraft(source()), [
    cut(2000, 3000),
    cut(2500, 3500),
    cut(3500, 4000),
    cut(-1000, 500),
    cut(9500, 12000),
  ]);
  assert.deepEqual(draft.edit.cuts, [
    { startFrame: 0, endFrame: 500 },
    { startFrame: 2000, endFrame: 4000 },
    { startFrame: 9500, endFrame: 10_000 },
  ]);
  assert.deepEqual(projectVoiceDraft(draft).keptRanges, [
    { startFrame: 500, endFrame: 2000 },
    { startFrame: 4000, endFrame: 9500 },
  ]);
  const original = createVoiceDraft(source(), transcript);
  assert.equal(
    projectVoiceDraft(editVoiceDraft(original, cut(1000, 2000))).transcript,
    'Keep, Bye!',
  );
  assert.equal(
    projectVoiceDraft(editVoiceDraft(original, cut(2000, 3000))).transcript,
    'Keep, secret. Bye!',
  );
  assert.strictEqual(editVoiceDraft(original, cut(1000, 1000)), original);
});

test('invalid frame intent is rejected atomically without damaging earlier edits', () => {
  const original = createVoiceDraft(source());
  for (const bad of [cut(NaN, 5), cut(1, Infinity), cut(10, 5), trim(8, 2), cut(0.1, 10)])
    assert.throws(() => editVoiceDraft(original, [cut(1000, 2000), bad]), hasCode('invalid_range'));
  assert.equal(projectVoiceDraft(original).frameCount, 10_000);
  assert.equal(original.undo.length, 0);
});

test('trim can expand again without losing independent cuts', () => {
  let draft = editVoiceDraft(createVoiceDraft(source()), cut(4000, 6000));
  draft = editVoiceDraft(draft, trim(2000, 8000));
  assert.equal(projectVoiceDraft(draft).frameCount, 4000);
  draft = editVoiceDraft(draft, trim(0, 10_000));
  assert.deepEqual(projectVoiceDraft(draft).keptRanges, [
    { startFrame: 0, endFrame: 4000 },
    { startFrame: 6000, endFrame: 10_000 },
  ]);
});

test('full-word retention drops a partially cut secret and rebases surviving timestamps', () => {
  const initial = createVoiceDraft(source(), transcript);
  const draft = editVoiceDraft(initial, cut(1500, 1750));
  const output = projectVoiceDraft(draft);
  assert.equal(projectVoiceDraft(initial).transcript, 'Keep, secret. Bye!');
  assert.equal(output.transcript, 'Keep, Bye!');
  assert.equal(output.frameCount, 9750);
  assert.deepEqual(output.words, [
    { id: 'keep', text: 'Keep,', startFrame: 0, endFrame: 1000 },
    { id: 'bye', text: 'Bye!', startFrame: 2750, endFrame: 3750 },
  ]);
  assert.ok(!output.transcript.includes('secret'));
  assert.equal(initial.transcript!.words[2]!.startFrame, 3000);
});

test('an edited clip cannot reuse stale untimed or incompletely aligned full text', () => {
  const draft = createVoiceDraft(source(), { text: 'untimed sensitive words', words: [] });
  assert.equal(projectVoiceDraft(draft).transcript, 'untimed sensitive words');
  assert.equal(projectVoiceDraft(editVoiceDraft(draft, cut(0, 1000))).transcript, '');
  const incomplete = createVoiceDraft(source(), {
    text: 'Keep, secret.',
    words: [transcript.words[0]!],
  });
  assert.equal(projectVoiceDraft(editVoiceDraft(incomplete, cut(1500, 2000))).transcript, 'Keep,');
});
