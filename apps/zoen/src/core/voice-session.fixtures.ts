import {
  VoiceSession,
  type VoiceNativePort,
  type VoicePreparedSource,
  type VoiceRenderResult,
  type VoiceTranscription,
} from '../media/voice-session.ts';

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
export const source = (id = 'source_a'): VoicePreparedSource => ({
  id,
  uri: `file:///lease/${id}/original.m4a`,
  durationMs: 4000,
  sampleRate: 1000,
  frameCount: 4000,
  levels: [0.1, 0.3, 0.7, 1],
});
export const transcript: VoiceTranscription = {
  status: 'ready',
  text: 'Keep secret after',
  words: [
    { id: 'keep', text: 'Keep', startFrame: 0, endFrame: 1000 },
    { id: 'secret', text: 'secret', startFrame: 1000, endFrame: 2000 },
    { id: 'after', text: 'after', startFrame: 2500, endFrame: 3500 },
  ],
};
export const cut = (startFrame: number, endFrame: number) => ({
  type: 'cut' as const,
  range: { startFrame, endFrame },
});
export function rendered(uri: string, frames: number): VoiceRenderResult {
  return { uri, mime: 'audio/mp4', durationMs: frames, frameCount: frames, levels: [0.2, 0.8] };
}
export function fixture() {
  const calls: string[] = [];
  let exports = 0;
  const port: VoiceNativePort = {
    prepare: async () => source(),
    render: async (id, ranges) => {
      calls.push(`render:${id}`);
      return rendered(
        `file:///render/${++exports}.m4a`,
        ranges.reduce((sum, range) => sum + range.endFrame - range.startFrame, 0),
      );
    },
    transcribe: async (id, locale) => {
      calls.push(`transcribe:${id}:${locale}`);
      return transcript;
    },
    releaseRender: async (id, uri) => {
      calls.push(`render-release:${id}:${uri}`);
    },
    cancel: async (id) => {
      calls.push(`cancel:${id}`);
    },
    release: async (id) => {
      calls.push(`release:${id}`);
    },
  };
  const session = new VoiceSession(port);
  return { port, calls, session };
}
