import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  VoiceSession,
  VoiceSessionFailure,
  type VoiceNativePort,
  type VoicePreparedSource,
  type VoiceRenderResult,
  type VoiceTranscription,
} from '../media/voice-session.ts';
import type { FrameRange } from '../media/voice-edit.ts';
import { voiceEditorState } from '../media/voice-editor-state.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
const source = (id = 'source_a'): VoicePreparedSource => ({
  id,
  uri: `file:///lease/${id}/original.m4a`,
  durationMs: 4000,
  sampleRate: 1000,
  frameCount: 4000,
  levels: [0.1, 0.3, 0.7, 1],
});
const transcript: VoiceTranscription = {
  status: 'ready',
  text: 'Keep secret after',
  words: [
    { id: 'keep', text: 'Keep', startFrame: 0, endFrame: 1000 },
    { id: 'secret', text: 'secret', startFrame: 1000, endFrame: 2000 },
    { id: 'after', text: 'after', startFrame: 2500, endFrame: 3500 },
  ],
};
const cut = (startFrame: number, endFrame: number) => ({
  type: 'cut' as const,
  range: { startFrame, endFrame },
});
function rendered(uri: string, frames: number): VoiceRenderResult {
  return { uri, mime: 'audio/mp4', durationMs: frames, frameCount: frames, levels: [0.2, 0.8] };
}
function fixture() {
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

test('a finalized source remains preparing and unsendable until its private ingestion is durable', async () => {
  const f = fixture();
  const entered = deferred<void>();
  const persisted = deferred<void>();
  const preparing = f.session.prepare('capture', {
    logicalSourceId: 'durable_source',
    beforeReady: async (prepared, editor) => {
      assert.equal(prepared.id, 'source_a');
      assert.equal(editor.draft.original.id, 'durable_source');
      assert.equal(JSON.stringify(editor).includes('file:'), false);
      entered.resolve();
      await persisted.promise;
    },
  });
  await entered.promise;
  assert.equal(f.session.getSnapshot().phase, 'preparing');
  assert.equal(f.session.getSendable(), null);
  assert.equal(
    f.calls.some((call) => call.startsWith('render:')),
    false,
  );
  persisted.resolve();
  await preparing;
  assert.equal(f.session.getSnapshot().phase, 'ready');
  assert.ok(f.session.getSendable());
  await f.session.close();
});

test('cold editor restoration preserves word identities, undo and unapplied trim while using a new native lease', async () => {
  const previous = fixture();
  await previous.session.prepare('capture', { logicalSourceId: 'durable_source' });
  await previous.session.transcribe('en');
  previous.session.edit(cut(1000, 2000));
  await previous.session.retryPreview();
  const draft = previous.session.getSnapshot().draft!;
  previous.session.setPendingTrim(draft, { startFrame: 200, endFrame: 3500 });
  const saved = JSON.parse(JSON.stringify(previous.session.editorState()));
  await previous.session.close();
  const restored = fixture();
  restored.port.prepare = async () => source('new_native_lease');
  await restored.session.prepare('restored_private_copy', { editor: saved });
  const state = restored.session.getSnapshot();
  assert.equal(state.source?.id, 'new_native_lease');
  assert.equal(state.draft?.original.id, 'durable_source');
  assert.deepEqual(
    state.draft?.transcript?.words.map((word) => word.id),
    ['keep', 'secret', 'after'],
  );
  assert.equal(state.draft?.undo.length, 1);
  assert.deepEqual(state.pendingTrim, { startFrame: 200, endFrame: 3500 });
  assert.equal(
    restored.session.getSendable(),
    null,
    'pending sliders never publish the old canonical audio',
  );
  assert.ok(restored.calls.includes('render:new_native_lease'));
  restored.session.setPendingTrim(state.draft!, null);
  assert.equal(restored.session.getSendable()?.preview.frameCount, 3000);
  restored.session.undo();
  await restored.session.retryPreview();
  assert.equal(restored.session.getSendable()?.preview.frameCount, 4000);
  await restored.session.close();
});

test('a recovered attempted voice plays its exact frozen export and cannot edit or rerender its intent', async () => {
  const f = fixture();
  const prior = fixture();
  await prior.session.prepare('capture');
  const editor = voiceEditorState(prior.session.getSnapshot().draft!, null);
  await prior.session.close();
  const frozen = rendered('file:///owned/frozen.m4a', 4000);
  await f.session.prepare('restored', { editor, attempted: true, frozenPreview: frozen });
  assert.strictEqual(f.session.getSendable()?.preview, frozen);
  assert.equal(
    f.calls.some((call) => call.startsWith('render:')),
    false,
  );
  assert.throws(() => f.session.edit(cut(0, 1000)), VoiceSessionFailure);
  assert.throws(() => f.session.undo(), VoiceSessionFailure);
  await f.session.close();
});

test('a saved editor cannot be rebound to different decoded source frames', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  const editor = f.session.editorState();
  await f.session.close();
  const changed = fixture();
  changed.port.prepare = async () => ({ ...source('different'), frameCount: 3000 });
  await changed.session.prepare('replacement', { editor });
  assert.equal(changed.session.getSnapshot().phase, 'idle');
  assert.equal(changed.session.getSnapshot().error?.kind, 'prepare');
  assert.equal(changed.session.getSendable(), null);
  assert.ok(changed.calls.includes('release:different'));
  await changed.session.close();
});

test('a pending precision transaction blocks unrelated cuts and history until exact Apply or Discard', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  await f.session.transcribe('en');
  f.session.edit({ type: 'trim', range: { startFrame: 0, endFrame: 3500 } });
  f.session.edit(cut(2000, 2200));
  f.session.undo();
  await f.session.retryPreview();
  const applied = f.session.getSnapshot();
  const pending = { startFrame: 200, endFrame: 3200 };
  f.session.setPendingTrim(applied.draft!, pending);
  for (const action of [
    () => f.session.undo(),
    () => f.session.redo(),
    () => f.session.edit(cut(1000, 2000)),
    () => f.session.edit({ type: 'setWordRemoved', wordId: 'secret', removed: true }),
    () => f.session.edit({ type: 'restore', range: { startFrame: 0, endFrame: 4000 } }),
    () => f.session.edit({ type: 'trim', range: { startFrame: 100, endFrame: 3000 } }),
    () => f.session.edit([{ type: 'trim', range: pending }, cut(1000, 2000)]),
  ]) {
    assert.throws(action, VoiceSessionFailure);
    assert.strictEqual(f.session.getSnapshot().draft, applied.draft);
    assert.strictEqual(f.session.getSnapshot().source, applied.source);
    assert.deepEqual(f.session.getSnapshot().pendingTrim, pending);
  }
  f.session.edit({ type: 'trim', range: pending });
  await f.session.retryPreview();
  assert.equal(f.session.getSnapshot().pendingTrim, null);
  assert.deepEqual(f.session.getSnapshot().draft!.edit.trim, pending);
  f.session.undo();
  await f.session.retryPreview();
  assert.deepEqual(f.session.getSnapshot().draft!.edit, applied.draft!.edit);
  f.session.setPendingTrim(f.session.getSnapshot().draft!, pending);
  f.session.setPendingTrim(f.session.getSnapshot().draft!, null);
  f.session.redo();
  await f.session.retryPreview();
  assert.equal(f.session.getSnapshot().pendingTrim, null);
  assert.deepEqual(f.session.getSnapshot().draft!.edit.trim, pending);
  await f.session.close();
});

test('transcript metadata completion preserves an unapplied precision range and retires old discard callbacks', async () => {
  const f = fixture();
  const recognition = deferred<VoiceTranscription>();
  f.port.transcribe = async () => recognition.promise;
  await f.session.prepare('capture');
  f.session.edit({ type: 'trim', range: { startFrame: 0, endFrame: 3500 } });
  await f.session.retryPreview();
  const oldDraft = f.session.getSnapshot().draft!;
  const pending = { startFrame: 200, endFrame: 3200 };
  const transcribing = f.session.transcribe('en');
  f.session.setPendingTrim(oldDraft, pending);
  recognition.resolve(transcript);
  await transcribing;
  const current = f.session.getSnapshot();
  assert.notStrictEqual(current.draft, oldDraft);
  assert.equal(current.draft!.transcript!.words.length, 3);
  assert.deepEqual(current.pendingTrim, pending);
  f.session.setPendingTrim(oldDraft, null);
  assert.deepEqual(f.session.getSnapshot().pendingTrim, pending);
  assert.throws(() => f.session.undo(), VoiceSessionFailure);
  f.session.edit({ type: 'trim', range: pending });
  await f.session.retryPreview();
  assert.equal(f.session.getSnapshot().pendingTrim, null);
  assert.equal(f.session.getSnapshot().draft!.transcript!.words.length, 3);
  await f.session.close();
});

test('prepare keeps an immutable native descriptor and only a current validated preview is sendable', async () => {
  const f = fixture();
  const input = { ...source(), levels: [0.2, 0.7] };
  f.port.prepare = async () => input;
  await f.session.prepare('file:///expo/capture.m4a');
  input.levels[0] = 1;
  assert.equal(f.session.getSnapshot().source?.levels[0], 0.2);
  assert.equal(f.session.getSnapshot().source?.id, 'source_a');
  assert.ok(Object.isFrozen(f.session.getSnapshot().source));
  assert.ok(Object.isFrozen(f.session.getSnapshot().preview?.levels));
  assert.equal(f.session.getSendable()?.transcript, '');
  assert.equal(
    f.calls.some((call) => call.startsWith('transcribe:')),
    false,
  );
  await f.session.close();
  assert.equal(f.session.getSnapshot().phase, 'closed');
  assert.equal(f.session.getSnapshot().draft, null);
  assert.equal(f.session.getSendable(), null);
});

test('committed edits invalidate send immediately and coalesce into one serial latest-preview export', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  const first = deferred<VoiceRenderResult>();
  const firstStarted = deferred<void>();
  const requests: FrameRange[][] = [];
  let active = 0,
    maxActive = 0,
    count = 0;
  f.port.render = async (_id, ranges) => {
    ++active;
    maxActive = Math.max(maxActive, active);
    requests.push([...ranges]);
    const index = ++count;
    if (index === 1) {
      firstStarted.resolve();
      const value = await first.promise;
      --active;
      return value;
    }
    --active;
    return rendered(
      `file:///latest-${index}.m4a`,
      ranges.reduce((sum, range) => sum + range.endFrame - range.startFrame, 0),
    );
  };
  f.session.edit(cut(1000, 1500));
  assert.equal(f.session.getSendable(), null);
  await firstStarted.promise;
  f.session.edit(cut(1500, 2000));
  f.session.edit(cut(2000, 2500));
  first.resolve(rendered('file:///stale.m4a', 3500));
  await f.session.retryPreview();
  assert.equal(count, 2);
  assert.equal(maxActive, 1);
  assert.deepEqual(requests[1], [
    { startFrame: 0, endFrame: 1000 },
    { startFrame: 2500, endFrame: 4000 },
  ]);
  assert.equal(f.session.getSendable()?.preview.frameCount, 2500);
  assert.ok(f.calls.includes('render-release:source_a:file:///stale.m4a'));
  assert.ok(f.calls.includes('render-release:source_a:file:///render/1.m4a'));
  await f.session.close();
});

test('a failed obsolete export cannot swallow a newer committed edit or replace its error state', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  const slow = deferred<VoiceRenderResult>();
  const started = deferred<void>();
  let calls = 0;
  f.port.render = async (_id, ranges) => {
    if (++calls === 1) {
      started.resolve();
      return slow.promise;
    }
    return rendered(
      'file:///fresh.m4a',
      ranges.reduce((sum, range) => sum + range.endFrame - range.startFrame, 0),
    );
  };
  f.session.edit(cut(1000, 1500));
  await started.promise;
  f.session.edit(cut(2000, 2500));
  slow.reject(new Error('obsolete export failed'));
  await f.session.retryPreview();
  assert.equal(f.session.getSnapshot().error, null);
  assert.equal(f.session.getSendable()?.preview.frameCount, 3000);
  await f.session.close();
});

test('preview failure keeps the original, edit history and old preview for a safe retry', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  const original = f.session.getSnapshot().source;
  const oldPreview = f.session.getSnapshot().preview;
  f.port.render = async () => {
    throw new Error('encoder unavailable');
  };
  f.session.edit(cut(1000, 2000));
  await f.session.retryPreview();
  assert.strictEqual(f.session.getSnapshot().source, original);
  assert.strictEqual(f.session.getSnapshot().preview, oldPreview);
  assert.equal(f.session.getSnapshot().draft?.undo.length, 1);
  assert.equal(f.session.getSnapshot().error?.kind, 'render');
  assert.equal(f.session.getSendable(), null);
  f.port.render = async () => rendered('file:///retry.m4a', 3000);
  await f.session.retryPreview();
  assert.equal(f.session.getSendable()?.preview.uri, 'file:///retry.m4a');
  f.session.undo();
  f.port.render = async () => rendered('file:///undo.m4a', 4000);
  await f.session.retryPreview();
  assert.equal(f.session.getSendable()?.preview.frameCount, 4000);
  await f.session.close();
});

test('wrong-frame, short, invalid-waveform and source-alias renders can never become sendable', async () => {
  for (const invalid of [
    { frameCount: 3999 },
    { durationMs: 249 },
    { durationMs: NaN },
    { mime: 'audio/webm' },
    { levels: [NaN] },
    { levels: Array(161).fill(1) },
    { uri: source().uri },
  ]) {
    const f = fixture();
    f.port.render = async () => ({ ...rendered('file:///invalid.m4a', 4000), ...invalid });
    await f.session.prepare('capture');
    assert.equal(f.session.getSnapshot().error?.kind, 'invalid_render');
    assert.equal(f.session.getSendable(), null);
    await f.session.close();
    assert.equal(
      f.calls.some((call) => call === `render-release:source_a:${source().uri}`),
      false,
    );
  }
});

test('an empty edit stays undoable without exporting or sending the retained old preview', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  const count = f.calls.filter((call) => call.startsWith('render:')).length;
  f.session.edit(cut(0, 4000));
  await f.session.retryPreview();
  assert.equal(f.session.getSnapshot().projection?.frameCount, 0);
  assert.equal(f.session.getSendable(), null);
  assert.equal(f.calls.filter((call) => call.startsWith('render:')).length, count);
  f.session.undo();
  await f.session.retryPreview();
  assert.equal(f.session.getSendable()?.preview.frameCount, 4000);
  await f.session.close();
});

test('explicit late transcription uses original coordinates and only surviving rebased text enters a send capture', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  const recognition = deferred<VoiceTranscription>();
  let requested = '';
  f.port.transcribe = async (_id, locale) => {
    requested = locale;
    return recognition.promise;
  };
  const transcribing = f.session.transcribe('pt-BR');
  f.session.edit(cut(1500, 1750));
  await f.session.retryPreview();
  recognition.resolve(transcript);
  await transcribing;
  assert.equal(requested, 'pt-BR');
  assert.equal(f.session.getSnapshot().draft?.transcript?.words[2]?.startFrame, 2500);
  assert.equal(f.session.getSnapshot().projection?.words[1]?.startFrame, 2250);
  const capture = f.session.getSendable();
  assert.equal(capture?.transcript, 'Keep after');
  assert.equal(JSON.stringify(capture).includes('secret'), false);
  f.session.edit(cut(0, 1000));
  assert.equal(f.session.getSendable(), null);
  assert.equal(capture?.transcript, 'Keep after');
  assert.ok(Object.isFrozen(capture));
  await f.session.retryPreview();
  assert.equal(f.session.getSendable()?.transcript, 'after');
  await f.session.close();
});

test('unavailable or failed explicit speech recognition preserves audio send and supports retry', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  f.port.transcribe = async () => ({ status: 'unavailable', text: '', words: [] });
  await f.session.transcribe('en-US');
  assert.equal(f.session.getSnapshot().transcription, 'unavailable');
  assert.equal(f.session.getSendable()?.transcript, '');
  f.port.transcribe = async () => {
    throw new Error('recognition failed');
  };
  await f.session.transcribe('en-US');
  assert.equal(f.session.getSnapshot().error?.kind, 'transcribe');
  assert.equal(f.session.getSendable()?.transcript, '');
  f.port.transcribe = async () => transcript;
  await f.session.transcribe('en-US');
  assert.equal(f.session.getSendable()?.transcript, 'Keep secret after');
  await f.session.close();
});

test('closing while prepare is pending awaits the late lease and releases it without publishing or exporting', async () => {
  const f = fixture();
  const preparing = deferred<VoicePreparedSource>();
  f.port.prepare = () => preparing.promise;
  const open = f.session.prepare('capture');
  let closed = false;
  const close = f.session.close().then(() => {
    closed = true;
  });
  await Promise.resolve();
  assert.equal(closed, false);
  preparing.resolve(source());
  await Promise.all([open, close]);
  assert.equal(f.session.getSnapshot().phase, 'closed');
  assert.equal(
    f.calls.some((call) => call.startsWith('render:')),
    false,
  );
  assert.ok(f.calls.includes('release:source_a'));
});

test('latest pending prepare wins and the obsolete source is released rather than adopted', async () => {
  const f = fixture();
  const first = deferred<VoicePreparedSource>();
  let prepares = 0;
  f.port.prepare = () => (++prepares === 1 ? first.promise : Promise.resolve(source('source_b')));
  const obsolete = f.session.prepare('first');
  await f.session.prepare('second');
  first.resolve(source('source_a'));
  await obsolete;
  assert.equal(f.session.getSnapshot().source?.id, 'source_b');
  assert.ok(f.calls.includes('release:source_a'));
  assert.equal(
    f.calls.some((call) => call === 'render:source_a'),
    false,
  );
  await f.session.close();
});

test('close cancels and drains active render and transcription before releasing original lease', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  const exportResult = deferred<VoiceRenderResult>();
  const recognition = deferred<VoiceTranscription>();
  const started = deferred<void>();
  f.port.render = async () => {
    started.resolve();
    return exportResult.promise;
  };
  f.port.transcribe = () => recognition.promise;
  const speech = f.session.transcribe('en-US');
  f.session.edit(cut(1000, 2000));
  await started.promise;
  const close = f.session.close();
  assert.equal(f.session.getSendable(), null);
  assert.equal(f.calls.includes('release:source_a'), false);
  exportResult.resolve(rendered('file:///late.m4a', 3000));
  recognition.resolve(transcript);
  await Promise.all([close, speech]);
  assert.equal(f.session.getSnapshot().draft, null);
  assert.equal(f.session.getSnapshot().transcription, 'idle');
  assert.ok(
    f.calls.indexOf('render-release:source_a:file:///late.m4a') <
      f.calls.indexOf('release:source_a'),
  );
});

test('failed final cleanup is visible, keeps ownership, and can be retried after cancellation failure', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  let releaseFails = true;
  f.port.cancel = async () => {
    throw new Error('cancellation failed');
  };
  f.port.release = async (id) => {
    f.calls.push(`release:${id}`);
    if (releaseFails) throw new Error('disk busy');
  };
  await assert.rejects(
    f.session.close(),
    (error: unknown) => error instanceof VoiceSessionFailure && error.kind === 'cleanup',
  );
  assert.equal(f.session.getSnapshot().phase, 'closing');
  assert.equal(f.session.getSnapshot().error?.kind, 'cleanup');
  assert.equal(f.session.getSendable(), null);
  releaseFails = false;
  await f.session.close();
  assert.equal(f.session.getSnapshot().phase, 'closed');
  assert.equal(f.calls.filter((call) => call === 'release:source_a').length, 2);
});

test('failed obsolete-preview cleanup stays owned and retry can reuse the current valid render', async () => {
  const f = fixture();
  await f.session.prepare('capture');
  let fail = true;
  const originalRelease = f.port.releaseRender;
  f.port.releaseRender = async (id, uri) => {
    if (fail) throw new Error('preview busy');
    await originalRelease(id, uri);
  };
  f.session.edit(cut(1000, 2000));
  await f.session.retryPreview();
  assert.equal(f.session.getSnapshot().error?.kind, 'cleanup');
  assert.equal(f.session.getSendable(), null);
  const validPreview = f.session.getSnapshot().preview;
  const count = f.calls.filter((call) => call.startsWith('render:')).length;
  fail = false;
  await f.session.retryPreview();
  assert.strictEqual(f.session.getSendable()?.preview, validPreview);
  assert.equal(f.calls.filter((call) => call.startsWith('render:')).length, count);
  await f.session.close();
});

test('invalid prepared metadata is released and a later preparation can retry the same original capture', async () => {
  const f = fixture();
  f.port.prepare = async () => ({ ...source(), sampleRate: 0 });
  await f.session.prepare('capture');
  assert.equal(f.session.getSnapshot().phase, 'idle');
  assert.equal(f.session.getSnapshot().error?.kind, 'prepare');
  assert.ok(f.calls.includes('release:source_a'));
  f.port.prepare = async () => source('source_retry');
  await f.session.prepare('capture');
  assert.equal(f.session.getSendable()?.preview.frameCount, 4000);
  await f.session.close();
});
