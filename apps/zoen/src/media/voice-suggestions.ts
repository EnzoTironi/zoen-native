import { editVoiceDraft, projectVoiceDraft } from './voice-edit.ts';
import type { FrameRange, VoiceDraft, VoiceEditCommand } from './voice-edit';

export const VOICE_LONG_PAUSE_MS = 1200;
export const VOICE_RETAINED_PAUSE_MS = 300;
export const VOICE_FILLER_MAX_MS = 800;
export type VoiceSuggestionLanguage = 'en' | 'en-US' | 'pt-BR';
export type VoiceSuggestion = Readonly<{
  id: string;
  kind: 'filler' | 'pause';
  sourceId: string;
  /** An exact original-frame cut, not a guessed interval inside recognized text. */
  range: FrameRange;
  segmentIds: readonly string[];
}>;
export type VoiceSuggestions = Readonly<{
  fillers: readonly VoiceSuggestion[];
  pauses: readonly VoiceSuggestion[];
}>;
const englishFillers = new Set(['uh', 'um', 'uhm', 'umm', 'erm']);
// "um", "é", "tipo", "hum", "hmm" and "eh" often carry meaning; never suggest them.
const portugueseFillers = new Set(['ahn', 'ãhn', 'uhm', 'umm']);

function possibleFiller(text: string, language: VoiceSuggestionLanguage): boolean {
  if (/[?!¿¡"'“”‘’«»]/u.test(text)) return false;
  const bare = text
    .trim()
    .replace(/^[.,…]+|[.,…]+$/gu, '')
    .trim();
  if (!/^[\p{L}\p{M}]+$/u.test(bare)) return false;
  const lower = bare.toLowerCase();
  // Do not turn an acronym or mixed-case proper identifier into a hesitation.
  if (bare !== lower && bare !== `${lower[0]!.toUpperCase()}${lower.slice(1)}`) return false;
  const fillers = language === 'pt-BR' ? portugueseFillers : englishFillers;
  return fillers.has(lower);
}
function suggestion(
  draft: VoiceDraft,
  kind: VoiceSuggestion['kind'],
  startFrame: number,
  endFrame: number,
  segmentIds: readonly string[],
): VoiceSuggestion {
  return Object.freeze({
    id: JSON.stringify([draft.original.id, kind, startFrame, endFrame, ...segmentIds]),
    kind,
    sourceId: draft.original.id,
    range: Object.freeze({ startFrame, endFrame }),
    segmentIds: Object.freeze([...segmentIds]),
  });
}

/** Optional, explicit proposals only. Timing gaps may contain sound; they are not verified silence. */
export function getVoiceSuggestions(
  draft: VoiceDraft,
  language: VoiceSuggestionLanguage,
): VoiceSuggestions {
  const fragments = draft.transcript?.words ?? [];
  const projection = projectVoiceDraft(draft);
  const retained = new Set(projection.words.map((fragment) => fragment.id));
  const rate = draft.original.sampleRate;
  const maximumFillerFrames = Math.floor((rate * VOICE_FILLER_MAX_MS) / 1000);
  const minimumPauseFrames = Math.ceil((rate * VOICE_LONG_PAUSE_MS) / 1000);
  const keptPauseFrames = Math.ceil((rate * VOICE_RETAINED_PAUSE_MS) / 1000);
  const leftPadding = Math.floor(keptPauseFrames / 2);
  const rightPadding = keptPauseFrames - leftPadding;
  const fillers: VoiceSuggestion[] = [];
  const pauses: VoiceSuggestion[] = [];
  for (let index = 0; index < fragments.length; index++) {
    const current = fragments[index]!;
    if (!retained.has(current.id)) continue;
    if (
      current.endFrame - current.startFrame <= maximumFillerFrames &&
      possibleFiller(current.text, language)
    )
      fillers.push(suggestion(draft, 'filler', current.startFrame, current.endFrame, [current.id]));
    // Original adjacency matters: skipping a removed fragment would invent a pause over speech.
    const previous = fragments[index - 1];
    if (
      !previous ||
      !retained.has(previous.id) ||
      current.startFrame - previous.endFrame <= minimumPauseFrames
    )
      continue;
    if (
      !projection.keptRanges.some(
        (range) => range.startFrame <= previous.endFrame && current.startFrame <= range.endFrame,
      )
    )
      continue;
    pauses.push(
      suggestion(
        draft,
        'pause',
        previous.endFrame + leftPadding,
        current.startFrame - rightPadding,
        [previous.id, current.id],
      ),
    );
  }
  return Object.freeze({ fillers: Object.freeze(fillers), pauses: Object.freeze(pauses) });
}

/** Revalidate against the live source and EDL. Stale suggestions never toggle removed audio back on. */
export function voiceSuggestionCommands(
  draft: VoiceDraft,
  language: VoiceSuggestionLanguage,
  ids: readonly string[],
): readonly VoiceEditCommand[] {
  const selected = new Set(ids);
  const current = getVoiceSuggestions(draft, language);
  return Object.freeze(
    [...current.fillers, ...current.pauses]
      .filter((value) => selected.has(value.id))
      .map((value) => Object.freeze({ type: 'cut' as const, range: value.range })),
  );
}

/** One accepted batch is one undo step; no acceptance occurs while merely computing suggestions. */
export function acceptVoiceSuggestions(
  draft: VoiceDraft,
  language: VoiceSuggestionLanguage,
  ids: readonly string[],
): VoiceDraft {
  return editVoiceDraft(draft, voiceSuggestionCommands(draft, language, ids));
}
