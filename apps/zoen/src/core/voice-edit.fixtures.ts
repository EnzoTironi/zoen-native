import {
  VoiceEditError,
  type VoiceEditErrorCode,
  type VoiceSource,
  type VoiceTranscript,
} from '../media/voice-edit.ts';

export const source = (): VoiceSource => ({
  id: 'synthetic-original',
  sampleRate: 1000,
  frameCount: 10_000,
  levels: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1],
});
export const transcript: VoiceTranscript = {
  text: 'Keep, secret. Bye!',
  words: [
    { id: 'keep', text: 'Keep,', startFrame: 0, endFrame: 1000 },
    { id: 'secret', text: 'secret.', startFrame: 1000, endFrame: 2000 },
    { id: 'bye', text: 'Bye!', startFrame: 3000, endFrame: 4000 },
  ],
};
export const cut = (startFrame: number, endFrame: number) => ({
  type: 'cut' as const,
  range: { startFrame, endFrame },
});
export const trim = (startFrame: number, endFrame: number) => ({
  type: 'trim' as const,
  range: { startFrame, endFrame },
});
export const toggle = (wordId: string) => ({ type: 'toggleWord' as const, wordId });
export const setWordRemoved = (wordId: string, removed: boolean) => ({
  type: 'setWordRemoved' as const,
  wordId,
  removed,
});
export const restore = (startFrame: number, endFrame: number) => ({
  type: 'restore' as const,
  range: { startFrame, endFrame },
});
export const hasCode = (code: VoiceEditErrorCode) => (error: unknown) =>
  error instanceof VoiceEditError && error.code === code;
