import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, find, recording, stagedPreview, tick } from './voice-controls.fixtures.ts';

for (const presentation of ['dock', 'expanded', 'raw'] as const) {
  for (const interruption of ['background', 'blur', 'suspended', 'disabled'] as const) {
    test(`${presentation} retained Send during ${interruption} cannot publish or consume the retained recording`, async () => {
      const editable = presentation !== 'raw';
      const f = recording(editable);
      let captured: unknown;
      if (editable) {
        const { session } = await stagedPreview(f);
        captured = session.getSendable();
      } else {
        await find(f.render(), 'VoiceRecordButton').props.onStart(false);
        await find(f.render(), 'VoiceDock').props.onStop();
        await tick();
      }
      if (presentation === 'expanded') find(f.render(), 'VoiceDock').props.onExpand();
      const component = presentation === 'expanded' ? 'VoiceReview' : 'VoiceDock';
      const retained = find(f.render(), component).props;
      const session = retained.session;
      const preview = session?.getSendable()?.preview;
      if (interruption === 'background') f.background();
      else if (interruption === 'blur') f.app.blur();
      else f.render({ [interruption]: true });
      await Promise.allSettled([retained.onSend(captured)]);
      assert.deepEqual(f.sent, []);
      assert.deepEqual(f.discarded, []);
      if (session) assert.strictEqual(session.getSendable()?.preview, preview);
      if (interruption === 'background') f.background('active');
      else if (interruption === 'blur') f.app.focus();
      const resumed = find(f.render(), component).props;
      await resumed.onSend(captured);
      assert.deepEqual(
        f.sent,
        [4000],
        'only a fresh Send after returning publishes the same retained audio',
      );
      await tick();
    });
  }
}

for (const editable of [false, true]) {
  test(`Send interrupted while audio restoration drains preserves the draft (${editable ? 'edited' : 'raw'})`, async () => {
    const f = recording(editable);
    await find(f.render(), 'VoiceRecordButton').props.onStart(false);
    f.failRestore(true);
    await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /restore failure/);
    await tick();
    const retained = find(f.render(), 'VoiceDock').props;
    const captured = retained.session?.getSendable();
    const gate = deferred<void>();
    f.failRestore(false);
    f.waitRestore(gate.promise);
    const sending = retained.onSend(captured);
    await tick();
    f.app.blur();
    gate.resolve();
    await Promise.allSettled([sending]);
    assert.deepEqual(f.sent, []);
    assert.deepEqual(f.discarded, []);
    f.app.focus();
    await find(f.render(), 'VoiceDock').props.onSend(captured);
    assert.deepEqual(f.sent, [4000]);
    assert.equal(f.stops(), 1, 'restoring the audio mode must not re-finalize the capture');
    await tick();
  });
}
