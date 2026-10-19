/** Local, non-destructive voice editing. No bytes, files, SDK state or wire metadata. */
export const MAX_VOICE_SECONDS = 3_600;
export const MIN_VOICE_MILLISECONDS = 250;
export const MAX_VOICE_WAVEFORM_BARS = 160;

export type FrameRange = Readonly<{ startFrame: number; endFrame: number }>;
export type VoiceSource = Readonly<{
  id: string;
  sampleRate: number;
  frameCount: number;
  /** Uniform bins over the complete original recording, each normalized to 0…1. */
  levels: readonly number[];
}>;
/** One original timed transcript fragment; it may be a whole phrase. Never infer inner word times. */
export type VoiceWord = FrameRange & Readonly<{ id: string; text: string }>;
export type VoiceTranscript = Readonly<{ text: string; words: readonly VoiceWord[] }>;
export type VoiceEdit = Readonly<{
  trim: FrameRange;
  cuts: readonly FrameRange[];
  removedWords: readonly string[];
}>;
export type VoiceDraft = Readonly<{
  original: VoiceSource;
  transcript: VoiceTranscript | null;
  edit: VoiceEdit;
  undo: readonly VoiceEdit[];
  redo: readonly VoiceEdit[];
}>;
export type VoiceEditCommand =
  | Readonly<{ type: 'trim'; range: FrameRange }>
  | Readonly<{ type: 'cut'; range: FrameRange }>
  | Readonly<{ type: 'restore'; range: FrameRange }>
  | Readonly<{ type: 'setWordRemoved'; wordId: string; removed: boolean }>
  | Readonly<{ type: 'toggleWord'; wordId: string }>;
export type VoiceProjection = Readonly<{
  keptRanges: readonly FrameRange[];
  frameCount: number;
  durationMs: number;
  canSend: boolean;
  transcript: string;
  words: readonly VoiceWord[];
}>;
/** Send-admitted projection. Render keptRanges against original.id, never a prior export. */
export type VoiceRenderPlan = VoiceProjection & Readonly<{ canSend: true }>;
export type VoiceEditErrorCode =
  | 'invalid_source'
  | 'invalid_range'
  | 'invalid_transcript'
  | 'unknown_word'
  | 'source_changed'
  | 'transcript_already_attached'
  | 'too_short'
  | 'invalid_waveform';
export class VoiceEditError extends Error {
  readonly code: VoiceEditErrorCode;
  constructor(code: VoiceEditErrorCode) {
    super(code);
    this.code = code;
    this.name = 'VoiceEditError';
  }
}
const fail = (code: VoiceEditErrorCode): never => {
  throw new VoiceEditError(code);
};
const integer = (value: number) => Number.isSafeInteger(value);
const range = (startFrame: number, endFrame: number): FrameRange =>
  Object.freeze({ startFrame, endFrame });

function checkedRange(value: FrameRange, frames: number): FrameRange {
  if (!integer(value.startFrame) || !integer(value.endFrame) || value.startFrame > value.endFrame)
    fail('invalid_range');
  return range(
    Math.max(0, Math.min(frames, value.startFrame)),
    Math.max(0, Math.min(frames, value.endFrame)),
  );
}
function normalized(ranges: readonly FrameRange[], frames: number): readonly FrameRange[] {
  const sorted = ranges
    .map((value) => checkedRange(value, frames))
    .filter((value) => value.endFrame > value.startFrame)
    .sort((a, b) => a.startFrame - b.startFrame || a.endFrame - b.endFrame);
  const result: FrameRange[] = [];
  for (const value of sorted) {
    const previous = result.at(-1);
    if (previous && value.startFrame <= previous.endFrame)
      result[result.length - 1] = range(
        previous.startFrame,
        Math.max(previous.endFrame, value.endFrame),
      );
    else result.push(value);
  }
  return Object.freeze(result);
}
function frozenEdit(
  trim: FrameRange,
  cuts: readonly FrameRange[],
  removedWords: readonly string[],
): VoiceEdit {
  return Object.freeze({ trim, cuts, removedWords: Object.freeze([...removedWords].sort()) });
}
function sameEdit(a: VoiceEdit, b: VoiceEdit): boolean {
  return (
    a.trim.startFrame === b.trim.startFrame &&
    a.trim.endFrame === b.trim.endFrame &&
    a.cuts.length === b.cuts.length &&
    a.cuts.every(
      (r, i) => r.startFrame === b.cuts[i]!.startFrame && r.endFrame === b.cuts[i]!.endFrame,
    ) &&
    a.removedWords.length === b.removedWords.length &&
    a.removedWords.every((id, i) => id === b.removedWords[i])
  );
}
function checkedTranscript(value: VoiceTranscript, frames: number): VoiceTranscript {
  if (typeof value.text !== 'string') fail('invalid_transcript');
  const ids = new Set<string>();
  let previousEnd = 0;
  const words = value.words.map((word) => {
    if (
      typeof word.id !== 'string' ||
      !word.id ||
      ids.has(word.id) ||
      typeof word.text !== 'string' ||
      !word.text.trim() ||
      !integer(word.startFrame) ||
      !integer(word.endFrame) ||
      word.startFrame < previousEnd ||
      word.endFrame <= word.startFrame ||
      word.endFrame > frames
    )
      fail('invalid_transcript');
    ids.add(word.id);
    previousEnd = word.endFrame;
    return Object.freeze({
      id: word.id,
      text: word.text,
      startFrame: word.startFrame,
      endFrame: word.endFrame,
    });
  });
  return Object.freeze({ text: value.text, words: Object.freeze(words) });
}
export function createVoiceDraft(
  source: VoiceSource,
  transcript: VoiceTranscript | null = null,
): VoiceDraft {
  if (
    typeof source.id !== 'string' ||
    !source.id ||
    !integer(source.sampleRate) ||
    source.sampleRate <= 0 ||
    source.sampleRate > Math.floor(Number.MAX_SAFE_INTEGER / MAX_VOICE_SECONDS) ||
    !integer(source.frameCount) ||
    source.frameCount <= 0 ||
    source.frameCount > source.sampleRate * MAX_VOICE_SECONDS
  )
    fail('invalid_source');
  if (source.levels.some((level) => !Number.isFinite(level) || level < 0 || level > 1))
    fail('invalid_waveform');
  const original = Object.freeze({
    id: source.id,
    sampleRate: source.sampleRate,
    frameCount: source.frameCount,
    levels: Object.freeze([...source.levels]),
  });
  return Object.freeze({
    original,
    transcript: transcript ? checkedTranscript(transcript, source.frameCount) : null,
    edit: frozenEdit(range(0, source.frameCount), Object.freeze([]), []),
    undo: Object.freeze([]),
    redo: Object.freeze([]),
  });
}
/** A late on-device result may enrich this source once; it never replaces its audio or edits. */
export function attachVoiceTranscript(
  draft: VoiceDraft,
  sourceId: string,
  transcript: VoiceTranscript,
): VoiceDraft {
  if (sourceId !== draft.original.id) fail('source_changed');
  if (draft.transcript) fail('transcript_already_attached');
  return Object.freeze({
    ...draft,
    transcript: checkedTranscript(transcript, draft.original.frameCount),
  });
}
function keptRanges(draft: VoiceDraft, edit: VoiceEdit): readonly FrameRange[] {
  const wordCuts =
    draft.transcript?.words.filter((word) => edit.removedWords.includes(word.id)) ?? [];
  const removals = normalized([...edit.cuts, ...wordCuts], draft.original.frameCount);
  const kept: FrameRange[] = [];
  let cursor = edit.trim.startFrame;
  for (const cut of removals) {
    if (cut.endFrame <= cursor || cut.startFrame >= edit.trim.endFrame) continue;
    if (cut.startFrame > cursor) kept.push(range(cursor, cut.startFrame));
    cursor = Math.max(cursor, Math.min(edit.trim.endFrame, cut.endFrame));
  }
  if (cursor < edit.trim.endFrame) kept.push(range(cursor, edit.trim.endFrame));
  return Object.freeze(kept);
}
function wordVisible(ranges: readonly FrameRange[], word: VoiceWord): boolean {
  return ranges.some((r) => r.startFrame <= word.startFrame && word.endFrame <= r.endFrame);
}
function subtract(cuts: readonly FrameRange[], hole: FrameRange): readonly FrameRange[] {
  return Object.freeze(
    cuts.flatMap((cut) => {
      if (cut.endFrame <= hole.startFrame || cut.startFrame >= hole.endFrame) return [cut];
      const parts: FrameRange[] = [];
      if (cut.startFrame < hole.startFrame) parts.push(range(cut.startFrame, hole.startFrame));
      if (hole.endFrame < cut.endFrame) parts.push(range(hole.endFrame, cut.endFrame));
      return parts;
    }),
  );
}
function restoredEdit(draft: VoiceDraft, edit: VoiceEdit, restored: FrameRange): VoiceEdit {
  if (restored.startFrame === restored.endFrame) return edit;
  const overlaps = (word: VoiceWord) =>
    word.startFrame < restored.endFrame && restored.startFrame < word.endFrame;
  const words =
    draft.transcript?.words.filter(
      (word) => edit.removedWords.includes(word.id) && overlaps(word),
    ) ?? [];
  const restoredWords = new Set(words.map((word) => word.id));
  // A partial word restoration restores audio only. Its still-removed frames become ordinary
  // cuts so the whole word cannot silently reappear in audio or searchable metadata.
  const cuts = normalized([...edit.cuts, ...words], draft.original.frameCount);
  const gaps: FrameRange[] = [];
  if (restored.endFrame < edit.trim.startFrame)
    gaps.push(range(restored.endFrame, edit.trim.startFrame));
  if (edit.trim.endFrame < restored.startFrame)
    gaps.push(range(edit.trim.endFrame, restored.startFrame));
  return frozenEdit(
    range(
      Math.min(edit.trim.startFrame, restored.startFrame),
      Math.max(edit.trim.endFrame, restored.endFrame),
    ),
    normalized([...subtract(cuts, restored), ...gaps], draft.original.frameCount),
    edit.removedWords.filter((id) => !restoredWords.has(id)),
  );
}
function changedEdit(draft: VoiceDraft, edit: VoiceEdit, command: VoiceEditCommand): VoiceEdit {
  const frames = draft.original.frameCount;
  switch (command.type) {
    case 'trim':
      return frozenEdit(checkedRange(command.range, frames), edit.cuts, edit.removedWords);
    case 'cut':
      return frozenEdit(
        edit.trim,
        normalized([...edit.cuts, command.range], frames),
        edit.removedWords,
      );
    case 'restore':
      return restoredEdit(draft, edit, checkedRange(command.range, frames));
    case 'setWordRemoved':
    case 'toggleWord': {
      const word = draft.transcript?.words.find((candidate) => candidate.id === command.wordId);
      if (!word) return fail('unknown_word');
      const visible = wordVisible(keptRanges(draft, edit), word);
      const remove = command.type === 'setWordRemoved' ? command.removed : visible;
      if (remove)
        return visible ? frozenEdit(edit.trim, edit.cuts, [...edit.removedWords, word.id]) : edit;
      return visible ? edit : restoredEdit(draft, edit, word);
    }
  }
}
/** A complete gesture or multiple related commands form exactly one undo step. */
export function editVoiceDraft(
  draft: VoiceDraft,
  commands: VoiceEditCommand | readonly VoiceEditCommand[],
): VoiceDraft {
  let edit = draft.edit;
  for (const command of Array.isArray(commands) ? commands : [commands])
    edit = changedEdit(draft, edit, command);
  if (sameEdit(edit, draft.edit)) return draft;
  return Object.freeze({
    ...draft,
    edit,
    undo: Object.freeze([...draft.undo, draft.edit]),
    redo: Object.freeze([]),
  });
}
export function undoVoiceEdit(draft: VoiceDraft): VoiceDraft {
  const edit = draft.undo.at(-1);
  return edit
    ? Object.freeze({
        ...draft,
        edit,
        undo: Object.freeze(draft.undo.slice(0, -1)),
        redo: Object.freeze([...draft.redo, draft.edit]),
      })
    : draft;
}
export function redoVoiceEdit(draft: VoiceDraft): VoiceDraft {
  const edit = draft.redo.at(-1);
  return edit
    ? Object.freeze({
        ...draft,
        edit,
        undo: Object.freeze([...draft.undo, draft.edit]),
        redo: Object.freeze(draft.redo.slice(0, -1)),
      })
    : draft;
}
function originalToEdited(ranges: readonly FrameRange[], originalFrame: number): number {
  let offset = 0;
  for (const r of ranges) {
    if (originalFrame < r.startFrame) return offset;
    if (originalFrame < r.endFrame) return offset + originalFrame - r.startFrame;
    offset += r.endFrame - r.startFrame;
  }
  return offset;
}
/** Removed source positions map to the next kept boundary; the output end is a sentinel. */
export function originalToEditedFrame(
  projection: VoiceProjection,
  originalFrame: number,
): number | null {
  if (!Number.isFinite(originalFrame) || !projection.frameCount) return null;
  return originalToEdited(projection.keptRanges, originalFrame);
}
/** At a cut join choose the next kept source frame, never a removed source interval. */
export function editedToOriginalFrame(
  projection: VoiceProjection,
  editedFrame: number,
): number | null {
  if (!Number.isFinite(editedFrame) || !projection.frameCount) return null;
  let remaining = Math.max(0, Math.min(projection.frameCount, editedFrame));
  for (const kept of projection.keptRanges) {
    const length = kept.endFrame - kept.startFrame;
    if (remaining < length) return kept.startFrame + remaining;
    remaining -= length;
  }
  return projection.keptRanges.at(-1)?.endFrame ?? null;
}
/** Project only the retained intersection of an original selection; an empty selection is silent. */
export function voiceSelectionToEditedRange(
  projection: VoiceProjection,
  selection: FrameRange,
): FrameRange | null {
  if (
    !Number.isFinite(selection.startFrame) ||
    !Number.isFinite(selection.endFrame) ||
    selection.endFrame <= selection.startFrame ||
    !projection.frameCount
  )
    return null;
  let first: number | null = null;
  let last: number | null = null;
  for (const kept of projection.keptRanges) {
    const start = Math.max(kept.startFrame, selection.startFrame);
    const end = Math.min(kept.endFrame, selection.endFrame);
    if (start >= end) continue;
    first ??= originalToEdited(projection.keptRanges, start);
    last = originalToEdited(projection.keptRanges, end);
  }
  return first !== null && last !== null ? range(first, last) : null;
}
export function projectVoiceDraft(draft: VoiceDraft): VoiceProjection {
  const ranges = keptRanges(draft, draft.edit);
  const frames = ranges.reduce((sum, r) => sum + r.endFrame - r.startFrame, 0);
  const words = Object.freeze(
    (draft.transcript?.words ?? [])
      .filter((word) => wordVisible(ranges, word))
      .map((word) =>
        Object.freeze({
          ...word,
          startFrame: originalToEdited(ranges, word.startFrame),
          endFrame: originalToEdited(ranges, word.endFrame),
        }),
      ),
  );
  const untouched =
    ranges.length === 1 &&
    ranges[0]!.startFrame === 0 &&
    ranges[0]!.endFrame === draft.original.frameCount;
  return Object.freeze({
    keptRanges: ranges,
    frameCount: frames,
    durationMs: (frames / draft.original.sampleRate) * 1000,
    canSend: frames >= Math.ceil((draft.original.sampleRate * MIN_VOICE_MILLISECONDS) / 1000),
    transcript: untouched
      ? (draft.transcript?.text ?? '')
      : words.map((word) => word.text).join(' '),
    words,
  });
}
/** A caller may display/undo an empty edit, but may never export or send it. */
export function voiceRenderPlan(draft: VoiceDraft): VoiceRenderPlan {
  const projection = projectVoiceDraft(draft);
  if (!projection.canSend) fail('too_short');
  return Object.freeze({ ...projection, canSend: true });
}
/** Peak remapping of uniform original bins; no cut interval becomes an output bin. */
export function remapVoiceWaveform(draft: VoiceDraft, barCount = 40): readonly number[] {
  if (!integer(barCount) || barCount < 1 || barCount > MAX_VOICE_WAVEFORM_BARS)
    fail('invalid_waveform');
  const projection = projectVoiceDraft(draft);
  if (!projection.frameCount || !draft.original.levels.length) return Object.freeze([]);
  const levels = draft.original.levels;
  const result = Array.from({ length: barCount }, (_, index) => {
    const start = (index * projection.frameCount) / barCount;
    const end = ((index + 1) * projection.frameCount) / barCount;
    let offset = 0,
      peak = 0;
    for (const r of projection.keptRanges) {
      const length = r.endFrame - r.startFrame;
      const overlapStart = Math.max(start, offset),
        overlapEnd = Math.min(end, offset + length);
      if (overlapStart < overlapEnd) {
        const originalStart = r.startFrame + overlapStart - offset;
        const originalEnd = r.startFrame + overlapEnd - offset;
        const first = Math.max(
          0,
          Math.floor((originalStart / draft.original.frameCount) * levels.length),
        );
        const last = Math.min(
          levels.length,
          Math.ceil((originalEnd / draft.original.frameCount) * levels.length),
        );
        for (let bin = first; bin < last; bin++) peak = Math.max(peak, levels[bin]!);
      }
      offset += length;
    }
    return peak;
  });
  return Object.freeze(result);
}
