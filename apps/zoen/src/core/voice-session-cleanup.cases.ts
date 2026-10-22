import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  VoiceSessionFailure,
  type VoicePreparedSource,
  type VoiceRenderResult,
  type VoiceTranscription,
} from '../media/voice-session.ts';
import { cut, deferred, fixture, rendered, source, transcript } from './voice-session.fixtures.ts';

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
