import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { FrameRange } from '../media/voice-edit.ts';
import { type VoiceRenderResult, type VoiceTranscription } from '../media/voice-session.ts';
import { cut, deferred, fixture, rendered, source, transcript } from './voice-session.fixtures.ts';

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
