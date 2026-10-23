import assert from 'node:assert/strict';
import { test } from 'node:test';
import { type AudioClipPlayback } from '../media/audio-clip-controller.ts';
import { VoiceSession } from '../media/voice-session.ts';
import { deferred, fixture, tick, voicePort } from './audio-clip-playback.fixtures.ts';

test('real compiled provider preserves one playing native owner through compact/detail and status updates', async () => {
  const f = fixture();
  const session = new VoiceSession(voicePort());
  await session.prepare('original');
  const uri = session.getSendable()!.preview.uri;
  f.initial.set(uri, { isLoaded: true, duration: 10 });
  let binding: any;
  function Presentation({ compact }: { compact: boolean }) {
    binding = f.provider.useVoicePreviewPlayback();
    return binding
      ? f.jsx(f.controls.AudioClipControls, { playback: binding.playback, compact })
      : null;
  }
  const current = () => true;
  const tree = (compact: boolean) =>
    f.jsx(f.provider.VoicePreviewPlayback, {
      session,
      ownerKey: 'owner',
      disabled: false,
      isCurrent: current,
      children: f.jsx(Presentation, { compact }),
    });
  f.render(tree(true));
  const playback = binding.playback as AudioClipPlayback;
  await playback.playFrom(3.125);
  playback.setRate(1.5);
  f.native[0]!.emit({ currentTime: 3.4, playbackRate: 1.5, playing: true });
  const pauses = f.native[0]!.pauses;
  f.render(tree(false));
  assert.equal(f.native.length, 1, 'presentation changes allocate no additional player');
  assert.equal(binding.playback, playback);
  assert.equal(playback.getSnapshot().currentSeconds, 3.4);
  assert.equal(playback.getSnapshot().rate, 1.5);
  assert.equal(playback.getSnapshot().playing, true);
  assert.equal(
    f.native[0]!.pauses,
    pauses,
    'neither context re-publication nor status rendering pauses',
  );
  f.native[0]!.emit({ currentTime: 3.6 });
  f.render(tree(true));
  assert.equal(binding.playback, playback);
  assert.equal(f.native[0]!.pauses, pauses);
  f.unmount();
  await session.close();
});

test('real session pending trim synchronously revokes a delayed audition before source lease cleanup and ABA', async () => {
  const f = fixture();
  const session = new VoiceSession(voicePort());
  await session.prepare('original');
  f.initial.set(session.getSendable()!.preview.uri, { isLoaded: true, duration: 10 });
  let binding: any;
  function Presentation() {
    binding = f.provider.useVoicePreviewPlayback();
    return null;
  }
  f.render(
    f.jsx(f.provider.VoicePreviewPlayback, {
      session,
      ownerKey: 'owner',
      disabled: false,
      isCurrent: () => true,
      children: f.jsx(Presentation, {}),
    }),
  );
  const playback = binding.playback as AudioClipPlayback;
  const gate = deferred();
  f.native[0]!.setSeek(() => gate.promise);
  binding.playFromSelection(session.getSnapshot().draft!, { startFrame: 2000, endFrame: 4000 });
  await tick();
  assert.deepEqual(f.native[0]!.seeks, [2]);
  const draft = session.getSnapshot().draft!;
  session.setPendingTrim(draft, { startFrame: 1000, endFrame: 9000 });
  assert.equal(playback.getSnapshot().busy, false, 'revoked synchronously without a React render');
  session.setPendingTrim(draft, null);
  gate.resolve();
  await tick();
  assert.equal(f.native[0]!.plays, 0, 'discarding the trim cannot revive the retired command');
  assert.deepEqual(f.native[0]!.tolerances, [[0, 0]]);
  f.unmount();
  await session.close();
});

test('provider unmount disconnects before SDK passive release and late mode resolution never plays', async () => {
  const f = fixture();
  let playback!: AudioClipPlayback;
  function Owner() {
    playback = f.hook.useAudioClipPlayback({ uri: 'A' });
    return null;
  }
  f.initial.set('A', { isLoaded: true, duration: 10 });
  f.render(f.jsx(Owner, {}));
  const gate = deferred();
  f.mode(() => gate.promise);
  const pending = playback.playFrom();
  await tick();
  f.unmount();
  assert.equal(f.native[0]!.released, true);
  gate.resolve();
  await pending;
  await tick();
  assert.equal(f.native[0]!.plays, 0);
});
