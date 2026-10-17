import test from 'node:test';
import assert from 'node:assert/strict';
import {
  attachVoiceTranscript,
  createVoiceDraft,
  editVoiceDraft,
  projectVoiceDraft,
  redoVoiceEdit,
  remapVoiceWaveform,
  undoVoiceEdit,
  voiceRenderPlan,
  VoiceEditError,
  type VoiceEditErrorCode,
  type VoiceSource,
  type VoiceTranscript,
} from '../media/voice-edit.ts';
const source = (): VoiceSource => ({
  id: 'synthetic-original',
  sampleRate: 1000,
  frameCount: 10_000,
  levels: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1],
});
const transcript: VoiceTranscript = {
  text: 'Keep, secret. Bye!',
  words: [
    { id: 'keep', text: 'Keep,', startFrame: 0, endFrame: 1000 },
    { id: 'secret', text: 'secret.', startFrame: 1000, endFrame: 2000 },
    { id: 'bye', text: 'Bye!', startFrame: 3000, endFrame: 4000 },
  ],
};
const cut = (startFrame: number, endFrame: number) => ({
  type: 'cut' as const,
  range: { startFrame, endFrame },
});
const trim = (startFrame: number, endFrame: number) => ({
  type: 'trim' as const,
  range: { startFrame, endFrame },
});
const toggle = (wordId: string) => ({ type: 'toggleWord' as const, wordId });
const restore = (startFrame: number, endFrame: number) => ({
  type: 'restore' as const,
  range: { startFrame, endFrame },
});
const hasCode = (code: VoiceEditErrorCode) => (error: unknown) =>
  error instanceof VoiceEditError && error.code === code;

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

test('word toggle restores only that original word through waveform cuts and trims', () => {
  const original = createVoiceDraft(source(), transcript);
  let draft = editVoiceDraft(original, [cut(7000, 8000), cut(1200, 1800), trim(500, 9000)]);
  draft = editVoiceDraft(draft, toggle('secret'));
  assert.equal(projectVoiceDraft(draft).transcript, 'secret. Bye!');
  assert.deepEqual(draft.edit.cuts, [{ startFrame: 7000, endFrame: 8000 }]);
  draft = editVoiceDraft(draft, toggle('keep'));
  assert.equal(draft.edit.trim.startFrame, 0);
  assert.equal(projectVoiceDraft(draft).transcript, 'Keep, secret. Bye!');
  draft = editVoiceDraft(draft, toggle('secret'));
  assert.ok(!projectVoiceDraft(draft).transcript.includes('secret'));
  draft = editVoiceDraft(draft, toggle('secret'));
  assert.equal(projectVoiceDraft(draft).transcript, 'Keep, secret. Bye!');
  assert.deepEqual(draft.edit.cuts, [{ startFrame: 7000, endFrame: 8000 }]);
  assert.throws(() => editVoiceDraft(draft, toggle('missing')), hasCode('unknown_word'));
});

test('restoring a word outside the trim never restores intervening removed audio or words', () => {
  const initial = createVoiceDraft(source(), transcript);
  const late = editVoiceDraft(initial, trim(5000, 9000));
  const firstWord = editVoiceDraft(late, toggle('keep'));
  assert.deepEqual(projectVoiceDraft(firstWord).keptRanges, [
    { startFrame: 0, endFrame: 1000 },
    { startFrame: 5000, endFrame: 9000 },
  ]);
  assert.equal(projectVoiceDraft(firstWord).transcript, 'Keep,');
  assert.deepEqual(projectVoiceDraft(undoVoiceEdit(firstWord)), projectVoiceDraft(late));
  assert.deepEqual(
    projectVoiceDraft(redoVoiceEdit(undoVoiceEdit(firstWord))),
    projectVoiceDraft(firstWord),
  );

  const early = editVoiceDraft(initial, trim(0, 1000));
  const lastWord = editVoiceDraft(early, toggle('bye'));
  assert.deepEqual(projectVoiceDraft(lastWord).keptRanges, [
    { startFrame: 0, endFrame: 1000 },
    { startFrame: 3000, endFrame: 4000 },
  ]);
  assert.equal(projectVoiceDraft(lastWord).transcript, 'Keep, Bye!');
  assert.deepEqual(projectVoiceDraft(lastWord).words[1], {
    id: 'bye',
    text: 'Bye!',
    startFrame: 1000,
    endFrame: 2000,
  });
});

test('range restoration changes exactly the selected removed audio across trim, cuts and word removals', () => {
  const initial = createVoiceDraft(source(), transcript);
  let draft = editVoiceDraft(initial, [toggle('secret'), cut(3500, 8000), trim(2500, 9000)]);
  const before = projectVoiceDraft(draft);
  draft = editVoiceDraft(draft, restore(1500, 2800));
  assert.deepEqual(projectVoiceDraft(draft).keptRanges, [
    { startFrame: 1500, endFrame: 3500 },
    { startFrame: 8000, endFrame: 9000 },
  ]);
  assert.equal(projectVoiceDraft(draft).transcript, '');
  assert.deepEqual(projectVoiceDraft(undoVoiceEdit(draft)), before);
  draft = editVoiceDraft(draft, restore(1000, 1500));
  assert.equal(projectVoiceDraft(draft).transcript, 'secret.');
  assert.deepEqual(projectVoiceDraft(draft).words, [
    { id: 'secret', text: 'secret.', startFrame: 0, endFrame: 1000 },
  ]);
  const empty = editVoiceDraft(initial, trim(9000, 9000));
  assert.deepEqual(projectVoiceDraft(editVoiceDraft(empty, restore(-100, 1000))).keptRanges, [
    { startFrame: 0, endFrame: 1000 },
  ]);
  assert.strictEqual(editVoiceDraft(draft, restore(1000, 1000)), draft);
});

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
