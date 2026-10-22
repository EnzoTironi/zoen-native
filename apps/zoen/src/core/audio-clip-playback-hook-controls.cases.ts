import assert from 'node:assert/strict';
import { test } from 'node:test';
import { type AudioClipPlayback } from '../media/audio-clip-controller.ts';
import { deferred, fixture, tick } from './audio-clip-playback.fixtures.ts';

test('compiled hook binds initial/status state to the exact player and drops late A before passive release', async () => {
  const f = fixture();
  let playback!: AudioClipPlayback;
  function Owner({ uri }: { uri: string }) {
    playback = f.hook.useAudioClipPlayback({ uri });
    return null;
  }
  f.initial.set('A', { isLoaded: true, duration: 12, currentTime: 5, playbackRate: 1.5 });
  f.render(f.jsx(Owner, { uri: 'A' }));
  const previous = playback;
  assert.equal(playback.getSnapshot().loaded, true);
  assert.equal(playback.getSnapshot().currentSeconds, 5);
  const a = f.native[0]!;
  const oldCallback = a.allListeners[0]!;
  f.render(f.jsx(Owner, { uri: 'B' }), false);
  assert.notEqual(playback, previous);
  assert.equal(a.released, false, 'exercise the gap before passive SDK cleanup');
  assert.equal(playback.getSnapshot().loaded, false, 'B must not inherit A loaded status');
  assert.equal(a.listeners.size, 0);
  oldCallback({ ...a.state, currentTime: 9, duration: 25, isLoaded: true });
  assert.equal(playback.getSnapshot().loaded, false);
  assert.equal(playback.getSnapshot().durationSeconds, 0);
  assert.equal(playback.getSnapshot().currentSeconds, 0);
  await playback.playFrom(2);
  assert.equal(f.native[1]!.plays, 0);
  f.native[1]!.emit({ isLoaded: true, duration: 4, currentTime: 1 });
  assert.equal(playback.getSnapshot().durationSeconds, 4);
  f.render();
  const next = playback;
  f.render();
  assert.equal(playback, next, 'status renders retain the source facade');
  f.flush();
  assert.equal(a.released, true);
  f.unmount();
});

test('a failure that predates listener installation exposes Retry and retries only that same source', () => {
  const f = fixture();
  let playback!: AudioClipPlayback;
  function Owner() {
    playback = f.hook.useAudioClipPlayback({ uri: 'broken' });
    return f.jsx(f.controls.AudioClipControls, { playback });
  }
  f.initial.set('broken', { error: 'invalid audio', playbackState: 'failed' });
  f.render(f.jsx(Owner, {}));
  assert.equal(playback.getSnapshot().loadFailed, true);
  const retry = f
    .nodes()
    .find((node) => node.type === 'NativeButton' && node.props.label === 'media.retryAudio');
  assert.ok(retry);
  retry.props.onPress();
  assert.equal(f.native[0]!.replacements, 1);
  assert.equal(f.native[0]!.plays, 0);
  f.unmount();
});

test('compiled controls allow Pause to revoke pending mode/seek rather than disabling cancellation', async () => {
  const f = fixture();
  let playback!: AudioClipPlayback;
  function Owner() {
    playback = f.hook.useAudioClipPlayback({ uri: 'A' });
    return f.jsx(f.controls.AudioClipControls, { playback });
  }
  f.initial.set('A', { isLoaded: true, duration: 10 });
  f.render(f.jsx(Owner, {}));
  const mode = deferred();
  f.mode(() => mode.promise);
  const pending = playback.playFrom(2);
  await tick();
  f.render();
  const pause = f
    .nodes()
    .find(
      (node) => node.type === 'Pressable' && node.props.accessibilityLabel === 'media.pauseAudio',
    );
  assert.ok(pause);
  assert.equal(pause.props.disabled, false);
  pause.props.onPress();
  mode.resolve();
  await pending;
  await tick();
  assert.equal(f.native[0]!.plays, 0);
  assert.equal(playback.getSnapshot().busy, false);
  f.unmount();
});
