import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  createVoiceDraft,
  editVoiceDraft,
  projectVoiceDraft,
  undoVoiceEdit,
  redoVoiceEdit,
} from '../media/voice-edit.ts';
import type { VoiceTranscript, VoiceWord } from '../media/voice-edit';
import {
  acceptVoiceSuggestions,
  getVoiceSuggestions,
  voiceSuggestionCommands,
} from '../media/voice-suggestions.ts';

const fragments: readonly VoiceWord[] = [
  { id: 'hello', text: 'Hello', startFrame: 0, endFrame: 500 },
  { id: 'um', text: 'um,', startFrame: 500, endFrame: 800 },
  { id: 'phrase', text: 'we agree', startFrame: 2500, endFrame: 3500 },
  { id: 'uh', text: 'uh', startFrame: 5000, endFrame: 5300 },
  { id: 'end', text: 'the end.', startFrame: 7000, endFrame: 7500 },
];
function draft(words = fragments, id = 'original') {
  const transcript: VoiceTranscript = { text: words.map((word) => word.text).join(' '), words };
  return createVoiceDraft(
    { id, sampleRate: 1000, frameCount: 10_000, levels: [0, 0.5, 1] },
    transcript,
  );
}
const cut = (startFrame: number, endFrame: number) => ({
  type: 'cut' as const,
  range: { startFrame, endFrame },
});

test('suggestions are immutable proposals over complete original fragments, never edits or phrase splits', () => {
  const initial = draft();
  const suggestions = getVoiceSuggestions(initial, 'en-US');
  assert.deepEqual(
    suggestions.fillers.map((value) => value.segmentIds),
    [['um'], ['uh']],
  );
  assert.deepEqual(
    suggestions.pauses.map((value) => value.range),
    [
      { startFrame: 950, endFrame: 2350 },
      { startFrame: 3650, endFrame: 4850 },
      { startFrame: 5450, endFrame: 6850 },
    ],
  );
  assert.ok(Object.isFrozen(suggestions) && Object.isFrozen(suggestions.fillers));
  assert.ok(Object.isFrozen(suggestions.pauses[0]?.range));
  assert.equal(initial.undo.length, 0);
  assert.equal(projectVoiceDraft(initial).frameCount, 10_000);
  assert.deepEqual(
    getVoiceSuggestions(
      draft([{ id: 'phrase', text: 'um we agree', startFrame: 0, endFrame: 700 }]),
      'en',
    ).fillers,
    [],
  );
});

test('locale-specific filler policy rejects meaningful Portuguese words, questions, quotations and acronyms', () => {
  const texts = [
    'é',
    'tipo',
    'hum',
    'hmm',
    'eh',
    'UM',
    'um?',
    '“um”',
    'uh oh',
    'umbrage',
    'ER',
    'Um',
    'uh,',
    'ãhn',
    'AHN',
    'ahn!',
  ];
  const words = texts.map((text, index) => ({
    id: String(index),
    text,
    startFrame: index * 400,
    endFrame: index * 400 + 300,
  }));
  assert.deepEqual(
    getVoiceSuggestions(draft(words), 'en').fillers.map((value) => value.segmentIds[0]),
    ['11', '12'],
  );
  assert.deepEqual(
    getVoiceSuggestions(draft(words), 'pt-BR').fillers.map((value) => value.segmentIds[0]),
    ['13'],
  );
  assert.deepEqual(
    getVoiceSuggestions(draft([{ id: 'long', text: 'um', startFrame: 0, endFrame: 801 }]), 'en')
      .fillers,
    [],
  );
});

test('existing trims, partial fragment cuts and shortened gaps exclude unsafe or repeated proposals', () => {
  const partial = editVoiceDraft(draft(), cut(600, 700));
  const suggestions = getVoiceSuggestions(partial, 'en');
  assert.deepEqual(
    suggestions.fillers.map((value) => value.segmentIds[0]),
    ['uh'],
  );
  assert.equal(
    suggestions.pauses.some((value) => value.segmentIds.includes('um')),
    false,
  );
  const trimmed = editVoiceDraft(draft(), {
    type: 'trim',
    range: { startFrame: 600, endFrame: 7200 },
  });
  assert.deepEqual(
    getVoiceSuggestions(trimmed, 'en').fillers.map((value) => value.segmentIds[0]),
    ['uh'],
  );
  assert.equal(
    getVoiceSuggestions(trimmed, 'en').pauses.some((value) => value.segmentIds.includes('end')),
    false,
  );
  const gapCut = editVoiceDraft(draft(), cut(1000, 1100));
  assert.equal(
    getVoiceSuggestions(gapCut, 'en').pauses.some((value) => value.segmentIds.includes('um')),
    false,
  );
});

test('removing a fragment cannot fabricate a pause across its recognized speech', () => {
  const removed = editVoiceDraft(draft(), cut(500, 800));
  const pauses = getVoiceSuggestions(removed, 'en').pauses;
  assert.equal(pauses.length, 2);
  assert.equal(
    pauses.some((value) => value.segmentIds.includes('hello') || value.segmentIds.includes('um')),
    false,
  );
  assert.deepEqual(pauses[0]?.segmentIds, ['phrase', 'uh']);
});

test('one explicitly accepted mixed batch is one undo step and sends only surviving exact timed fragments', () => {
  const initial = draft();
  const proposals = getVoiceSuggestions(initial, 'en');
  const ids = [...proposals.fillers, ...proposals.pauses].map((value) => value.id);
  const accepted = acceptVoiceSuggestions(initial, 'en', [...ids, ids[0]!]);
  assert.equal(accepted.undo.length, 1);
  assert.equal(projectVoiceDraft(accepted).transcript, 'Hello we agree the end.');
  assert.deepEqual(
    projectVoiceDraft(accepted).words.map((word) => word.id),
    ['hello', 'phrase', 'end'],
  );
  assert.equal(projectVoiceDraft(accepted).words[1]?.text, 'we agree');
  const phrase = projectVoiceDraft(accepted).words[1];
  assert.ok(phrase);
  assert.equal(phrase.endFrame - phrase.startFrame, 1000);
  assert.deepEqual(projectVoiceDraft(undoVoiceEdit(accepted)), projectVoiceDraft(initial));
  assert.deepEqual(
    projectVoiceDraft(redoVoiceEdit(undoVoiceEdit(accepted))),
    projectVoiceDraft(accepted),
  );
  assert.deepEqual(voiceSuggestionCommands(accepted, 'en', ids), []);
  assert.strictEqual(acceptVoiceSuggestions(accepted, 'en', ids), accepted);
});

test('acceptance revalidates source and current retained regions instead of restoring stale suggestions', () => {
  const original = draft();
  const ids = getVoiceSuggestions(original, 'en').fillers.map((value) => value.id);
  const other = draft(fragments, 'different_source');
  assert.deepEqual(voiceSuggestionCommands(other, 'en', ids), []);
  const alreadyCut = editVoiceDraft(original, [cut(500, 800), cut(5000, 5300)]);
  assert.strictEqual(acceptVoiceSuggestions(alreadyCut, 'en', ids), alreadyCut);
  assert.deepEqual(
    projectVoiceDraft(alreadyCut).words.map((word) => word.id),
    ['hello', 'phrase', 'end'],
  );
});

test('pause boundaries use exact integer frames, retain300ms, and exclude leading/trailing unrecognized audio', () => {
  const initial = draft([
    { id: 'a', text: 'first phrase', startFrame: 1000, endFrame: 2000 },
    { id: 'b', text: 'next phrase', startFrame: 3200, endFrame: 4000 },
    { id: 'c', text: 'last phrase', startFrame: 5201, endFrame: 6000 },
  ]);
  const proposals = getVoiceSuggestions(initial, 'en');
  assert.equal(proposals.pauses.length, 1);
  assert.deepEqual(proposals.pauses[0]?.range, { startFrame: 4150, endFrame: 5051 });
  const applied = acceptVoiceSuggestions(
    initial,
    'en',
    proposals.pauses.map((value) => value.id),
  );
  assert.equal(projectVoiceDraft(applied).frameCount, 9099);
  assert.equal(projectVoiceDraft(applied).keptRanges[0]?.startFrame, 0);
  assert.equal(projectVoiceDraft(applied).keptRanges.at(-1)?.endFrame, 10_000);
  const odd = createVoiceDraft(
    { id: 'odd', sampleRate: 3, frameCount: 30, levels: [] },
    {
      text: 'a b',
      words: [
        { id: 'a', text: 'a', startFrame: 0, endFrame: 3 },
        { id: 'b', text: 'b', startFrame: 9, endFrame: 12 },
      ],
    },
  );
  assert.deepEqual(getVoiceSuggestions(odd, 'en').pauses[0]?.range, { startFrame: 3, endFrame: 8 });
});

test('missing or untimed transcription never creates guessed fillers or pauses from waveform', () => {
  const source = { id: 'untimed', sampleRate: 1000, frameCount: 4000, levels: [0, 0, 0, 0] };
  for (const transcript of [null, { text: 'um a long phrase uh', words: [] }])
    assert.deepEqual(getVoiceSuggestions(createVoiceDraft(source, transcript), 'en'), {
      fillers: [],
      pauses: [],
    });
});
