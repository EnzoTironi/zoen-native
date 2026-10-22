import assert from 'node:assert/strict';
import { test } from 'node:test';
import { voiceEditorState } from '../media/voice-editor-state.ts';
import {
  VoiceSessionFailure,
  type VoiceRenderResult,
  type VoiceTranscription,
} from '../media/voice-session.ts';
import { cut, deferred, fixture, rendered, source, transcript } from './voice-session.fixtures.ts';

for (const rejectedPhase of ['closing', 'closed'] as const) {
  test(`a throwing ${rejectedPhase} observer cannot abandon voice cancellation, drain or cleanup retry`, async (t) => {
    const f = fixture();
    await f.session.prepare('capture');
    const exportResult = deferred<VoiceRenderResult>();
    const recognition = deferred<VoiceTranscription>();
    const rendering = deferred<void>();
    const cancellation = deferred<void>();
    f.port.render = async () => {
      rendering.resolve();
      return exportResult.promise;
    };
    f.port.transcribe = () => recognition.promise;
    f.port.cancel = async (id) => {
      f.calls.push(`cancel:${id}`);
      cancellation.resolve();
    };
    let failRelease = true;
    f.port.release = async (id) => {
      f.calls.push(`release:${id}`);
      if (failRelease) throw Error('Owned recording file busy');
    };
    let refusedNotifications = 0;
    const healthyPhases: string[] = [];
    const offBad = f.session.subscribe(() => {
      if (f.session.getSnapshot().phase !== rejectedPhase) return;
      refusedNotifications++;
      throw Error('Synthetic voice control notification failure');
    });
    const offHealthy = f.session.subscribe(() => {
      healthyPhases.push(f.session.getSnapshot().phase);
    });
    t.after(async () => {
      offBad();
      offHealthy();
      failRelease = false;
      exportResult.resolve(rendered('file:///late-observer.m4a', 3000));
      recognition.resolve(transcript);
      await f.session.close();
    });
    const speech = f.session.transcribe('en-US');
    f.session.edit(cut(1000, 2000));
    await rendering.promise;
    let closing!: Promise<void>;
    assert.doesNotThrow(() => {
      closing = f.session.close();
    });
    await cancellation.promise;
    assert.equal(f.session.getSnapshot().phase, 'closing');
    assert.equal(f.session.getSendable(), null);
    assert.equal(f.calls.includes('release:source_a'), false, 'Native tasks still own the source');
    exportResult.resolve(rendered('file:///late-observer.m4a', 3000));
    recognition.resolve(transcript);
    await assert.rejects(closing, (error: unknown) => {
      assert.ok(error instanceof VoiceSessionFailure);
      assert.equal(error.kind, 'cleanup');
      assert.ok(error.cause instanceof AggregateError);
      assert.equal(error.cause.errors[0]?.message, 'Owned recording file busy');
      return true;
    });
    await speech;
    assert.equal(f.session.getSnapshot().error?.kind, 'cleanup');
    assert.ok(
      f.calls.indexOf('render-release:source_a:file:///late-observer.m4a') <
        f.calls.indexOf('release:source_a'),
    );
    failRelease = false;
    await f.session.close();
    assert.ok(refusedNotifications > 0);
    assert.ok(healthyPhases.includes('closing'));
    assert.equal(healthyPhases.at(-1), 'closed');
    assert.equal(f.session.getSnapshot().phase, 'closed');
    assert.equal(f.session.getSnapshot().source, null);
    assert.equal(f.session.getSnapshot().draft, null);
    assert.equal(f.calls.filter((call) => call === 'release:source_a').length, 2);
    await f.session.close();
    assert.equal(f.calls.filter((call) => call === 'release:source_a').length, 2);
  });
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
