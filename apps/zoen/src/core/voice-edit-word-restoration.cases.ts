import assert from 'node:assert/strict';
import test from 'node:test';
import {
  createVoiceDraft,
  editVoiceDraft,
  projectVoiceDraft,
  redoVoiceEdit,
  undoVoiceEdit,
} from '../media/voice-edit.ts';
import {
  cut,
  hasCode,
  restore,
  setWordRemoved,
  source,
  toggle,
  transcript,
  trim,
} from './voice-edit.fixtures.ts';

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

test('a queued word removal cannot restore a filler that a suggestion already cut', () => {
  const initial = createVoiceDraft(source(), {
    text: 'Keep, um Bye!',
    words: [
      { id: 'keep', text: 'Keep,', startFrame: 0, endFrame: 500 },
      { id: 'um', text: 'um', startFrame: 500, endFrame: 800 },
      { id: 'bye', text: 'Bye!', startFrame: 1000, endFrame: 1500 },
    ],
  });
  const accepted = editVoiceDraft(initial, cut(500, 800));
  const queued = editVoiceDraft(accepted, setWordRemoved('um', true));
  assert.strictEqual(queued, accepted);
  assert.equal(queued.undo.length, 1);
  assert.equal(projectVoiceDraft(queued).transcript, 'Keep, Bye!');
  assert.equal(projectVoiceDraft(queued).frameCount, 9700);

  const partial = editVoiceDraft(initial, cut(600, 700));
  assert.strictEqual(editVoiceDraft(partial, setWordRemoved('um', true)), partial);
  assert.equal(projectVoiceDraft(partial).frameCount, 9900);
  assert.equal(projectVoiceDraft(partial).transcript, 'Keep, Bye!');
});

test('desired word removal and restoration are idempotent without extra undo steps', () => {
  const initial = createVoiceDraft(source(), transcript);
  assert.strictEqual(editVoiceDraft(initial, setWordRemoved('secret', false)), initial);
  const removed = editVoiceDraft(initial, setWordRemoved('secret', true));
  assert.equal(removed.undo.length, 1);
  assert.equal(projectVoiceDraft(removed).transcript, 'Keep, Bye!');
  assert.strictEqual(editVoiceDraft(removed, setWordRemoved('secret', true)), removed);
  const restored = editVoiceDraft(removed, setWordRemoved('secret', false));
  assert.equal(restored.undo.length, 2);
  assert.equal(projectVoiceDraft(restored).transcript, transcript.text);
  assert.strictEqual(editVoiceDraft(restored, setWordRemoved('secret', false)), restored);
  assert.deepEqual(projectVoiceDraft(undoVoiceEdit(restored)), projectVoiceDraft(removed));
  assert.deepEqual(
    projectVoiceDraft(redoVoiceEdit(undoVoiceEdit(restored))),
    projectVoiceDraft(restored),
  );
  assert.throws(
    () => editVoiceDraft(initial, setWordRemoved('missing', true)),
    hasCode('unknown_word'),
  );
});

test('desired word restoration repairs cuts and trims while preserving unrelated removed audio', () => {
  const initial = createVoiceDraft(source(), transcript);
  const cutDraft = editVoiceDraft(initial, [cut(1200, 1800), cut(7000, 8000)]);
  const restoredCut = editVoiceDraft(cutDraft, setWordRemoved('secret', false));
  assert.equal(projectVoiceDraft(restoredCut).transcript, transcript.text);
  assert.deepEqual(restoredCut.edit.cuts, [{ startFrame: 7000, endFrame: 8000 }]);
  assert.strictEqual(editVoiceDraft(restoredCut, setWordRemoved('secret', false)), restoredCut);

  const trimmed = editVoiceDraft(initial, [trim(5000, 9000), cut(7000, 8000)]);
  const restoredTrim = editVoiceDraft(trimmed, setWordRemoved('secret', false));
  assert.deepEqual(projectVoiceDraft(restoredTrim).keptRanges, [
    { startFrame: 1000, endFrame: 2000 },
    { startFrame: 5000, endFrame: 7000 },
    { startFrame: 8000, endFrame: 9000 },
  ]);
  assert.equal(projectVoiceDraft(restoredTrim).transcript, 'secret.');
  assert.deepEqual(restoredTrim.edit.cuts, [
    { startFrame: 2000, endFrame: 5000 },
    { startFrame: 7000, endFrame: 8000 },
  ]);
  assert.deepEqual(projectVoiceDraft(undoVoiceEdit(restoredTrim)), projectVoiceDraft(trimmed));
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
