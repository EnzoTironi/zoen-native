import {
  createVoiceDraft,
  editVoiceDraft,
  MAX_VOICE_WAVEFORM_BARS,
  type FrameRange,
  type VoiceDraft,
  type VoiceEdit,
  type VoiceSource,
} from './voice-edit.ts';

/** Private editor recipe. Native leases and cache paths never enter this value. */
export type VoiceEditorState = Readonly<{
  version: 1;
  draft: VoiceDraft;
  pendingTrim: FrameRange | null;
}>;

function invalid(): never {
  throw new Error('The saved recording could not be recovered.');
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
}
function range(value: unknown, frames: number): FrameRange {
  const item = object(value);
  if (
    !Number.isSafeInteger(item.startFrame) ||
    !Number.isSafeInteger(item.endFrame) ||
    (item.startFrame as number) < 0 ||
    (item.endFrame as number) < (item.startFrame as number) ||
    (item.endFrame as number) > frames
  )
    return invalid();
  return Object.freeze({
    startFrame: item.startFrame as number,
    endFrame: item.endFrame as number,
  });
}
function edit(value: unknown, base: VoiceDraft): VoiceEdit {
  const item = object(value);
  if (!Array.isArray(item.cuts) || !Array.isArray(item.removedWords)) return invalid();
  if (item.cuts.length > 4096 || item.removedWords.length > 16384) return invalid();
  return editVoiceDraft(base, [
    { type: 'trim', range: range(item.trim, base.original.frameCount) },
    ...item.cuts.map((cut) => ({
      type: 'cut' as const,
      range: range(cut, base.original.frameCount),
    })),
    ...item.removedWords.map((id) => {
      if (typeof id !== 'string') return invalid();
      return { type: 'setWordRemoved' as const, wordId: id, removed: true };
    }),
  ]).edit;
}

/** Revalidates every saved range against the freshly decoded immutable source. */
export function restoreVoiceEditor(value: unknown, source: VoiceSource): VoiceEditorState {
  const saved = object(value);
  const draft = object(saved.draft);
  const original = object(draft.original);
  if (
    saved.version !== 1 ||
    typeof original.id !== 'string' ||
    !original.id ||
    original.id.length > 256 ||
    original.sampleRate !== source.sampleRate ||
    original.frameCount !== source.frameCount ||
    !Array.isArray(original.levels) ||
    original.levels.length > MAX_VOICE_WAVEFORM_BARS ||
    !Array.isArray(draft.undo) ||
    !Array.isArray(draft.redo) ||
    draft.undo.length + draft.redo.length > 4096
  )
    return invalid();
  const base = createVoiceDraft(
    { ...source, id: original.id, levels: original.levels as number[] },
    draft.transcript as VoiceDraft['transcript'],
  );
  const restored = Object.freeze({
    ...base,
    edit: edit(draft.edit, base),
    undo: Object.freeze(draft.undo.map((item) => edit(item, base))),
    redo: Object.freeze(draft.redo.map((item) => edit(item, base))),
  });
  return Object.freeze({
    version: 1,
    draft: restored,
    pendingTrim: saved.pendingTrim === null ? null : range(saved.pendingTrim, source.frameCount),
  });
}

export function voiceEditorState(
  draft: VoiceDraft,
  pendingTrim: FrameRange | null,
): VoiceEditorState {
  return restoreVoiceEditor({ version: 1, draft, pendingTrim }, draft.original);
}
