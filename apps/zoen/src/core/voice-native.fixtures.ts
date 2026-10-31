import type { VoiceNativePort } from '../media/voice-session.ts';

export function voicePort(): VoiceNativePort {
  let renders = 0;
  return {
    prepare: async () => ({
      id: 'synthetic-voice',
      uri: 'file:///voice/original.m4a',
      sampleRate: 1000,
      frameCount: 4000,
      durationMs: 4000,
      levels: [0.2, 0.8],
    }),
    render: async (_id, ranges) => {
      const durationMs = ranges.reduce((sum, range) => sum + range.endFrame - range.startFrame, 0);
      return {
        uri: `file:///voice/render-${++renders}.m4a`,
        frameCount: durationMs,
        durationMs,
        mime: 'audio/mp4',
        levels: [0.2, 0.8],
      };
    },
    transcribe: async () => ({ status: 'unavailable', text: '', words: [] }),
    releaseRender: async () => {},
    cancel: async () => {},
    release: async () => {},
  };
}
