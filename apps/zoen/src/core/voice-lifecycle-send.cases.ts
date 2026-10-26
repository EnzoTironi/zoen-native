import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  component,
  deferred,
  find,
  nodes,
  recording,
  stagedPreview,
  tick,
} from './voice-controls.fixtures.ts';

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

test('retired Send is silent in the real dock/review while genuine send errors stay visible', async () => {
  for (const expanded of [false, true]) {
    const f = recording(true);
    const restoration = expanded ? deferred<void>() : null;
    if (expanded) {
      await find(f.render(), 'VoiceRecordButton').props.onStart(false);
      f.failRestore(true);
      await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /restore failure/);
      await tick();
      f.failRestore(false);
      f.waitRestore(restoration!.promise);
    } else await stagedPreview(f);
    const session = find(f.render(), 'VoiceDock').props.session;
    const preview = session.getSendable()!.preview;
    if (expanded) find(f.render(), 'VoiceDock').props.onExpand();
    const presentation = expanded ? 'VoiceReview' : 'VoiceDock';
    const renderer = component(expanded ? 'voice-review.ios' : 'voice-dock', {});
    const button = expanded ? 'NativeButton' : 'ComposerAction';
    const retained = find(f.render(), presentation).props;
    const press = find(renderer.render(retained), button, 'media.send').props.onPress;
    if (restoration) {
      press();
      await tick();
      f.app.blur();
      restoration.resolve();
    } else {
      f.app.blur();
      press();
    }
    await tick();
    await tick();
    f.app.focus();
    const resumed = find(f.render(), presentation).props;
    const returned = renderer.render(resumed);
    assert.equal(
      nodes(returned).some(
        (node) => node.type === 'Text' && node.props.children === 'media.voiceRecordingUnavailable',
      ),
      false,
      'lifecycle cancellation must not paint a failure on a healthy retained draft',
    );
    assert.strictEqual(session.getSendable()!.preview, preview);
    assert.equal(session.getSnapshot().phase, 'ready');
    assert.strictEqual(
      find(f.render(), presentation).props.session,
      session,
      'silent cancellation cannot acknowledge or remove the parent draft',
    );
    assert.deepEqual(f.sent, []);
    assert.deepEqual(f.discarded, []);
    const failed = {
      ...resumed,
      onSend: async () => {
        throw new Error('Synthetic real send failure');
      },
    };
    find(renderer.render(failed), button, 'media.send').props.onPress();
    await tick();
    assert.ok(
      nodes(renderer.render(failed)).some(
        (node) => node.type === 'Text' && node.props.children === 'Synthetic real send failure',
      ),
    );
    find(renderer.render(resumed), button, 'media.send').props.onPress();
    await tick();
    await tick();
    assert.deepEqual(f.sent, [4000]);
    renderer.unmount();
  }
});
