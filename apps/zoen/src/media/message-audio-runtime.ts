import { createAudioPlayer, setAudioModeAsync } from 'expo-audio';
import { AudioClipController } from './audio-clip-controller.ts';
import { mediaPlayback } from './playback-owner.ts';
import type { MessageAudioRuntime, MessageAudioSource } from './message-audio-session.ts';
import type { LocalMediaFile } from './types.ts';

/** This adapter alone owns the non-hook SharedObject and its pending native operations. */
export function createMessageAudioRuntime(
  source: MessageAudioSource,
  file: LocalMediaFile,
  current: () => boolean,
): MessageAudioRuntime {
  const player = createAudioPlayer(
    { uri: file.uri },
    {
      updateInterval: 100,
      allowsExternalPlayback: false,
    },
  );
  const playback = new AudioClipController(source.durationMs, mediaPlayback, player);
  let live = true;
  let released = false;
  let requestedPlay = false;
  let setupFailure: Error | undefined;
  let releaseTask: Promise<void> | null = null;
  const pending = new Set<Promise<unknown>>();
  const track = <T>(task: Promise<T>): Promise<T> => {
    pending.add(task);
    void task.finally(() => pending.delete(task)).catch(() => {});
    return task;
  };
  let disconnect = () => {};
  let subscription: { remove(): void } | null = null;
  try {
    disconnect = playback.connect({
      playing: () => !released && player.playing,
      currentSeconds: () => (released ? 0 : player.currentTime),
      play: () => {
        if (live && current()) {
          requestedPlay = true;
          player.play();
        }
      },
      pause: () => {
        requestedPlay = false;
        if (!released) player.pause();
      },
      seek: (seconds) => (live ? track(player.seekTo(seconds, 0, 0)) : Promise.resolve()),
      setRate: (rate) => {
        if (live && current()) player.setPlaybackRate(rate);
      },
      reload: () => {
        if (live && current()) player.replace({ uri: file.uri });
      },
      preparePlayback: () =>
        live
          ? track(
              setAudioModeAsync({
                playsInSilentMode: true,
                allowsRecording: false,
                shouldPlayInBackground: false,
                interruptionMode: 'doNotMix',
              }),
            )
          : Promise.resolve(),
    });
    playback.configure(true, () => live && current(), source.durationMs);
    const playerId = player.id;
    subscription = player.addListener('playbackStatusUpdate', (status) => {
      if (!live || status.id !== playerId) return;
      if (status.didJustFinish || status.error || status.playbackState === 'failed') {
        requestedPlay = false;
        playback.pause();
      }
      playback.observe(status);
    });
    playback.observe(player.currentStatus);
  } catch (error) {
    // Return the owner even on setup failure: a failed native release must stay retryable.
    live = false;
    setupFailure =
      error instanceof Error ? error : new Error('Audio setup failed.', { cause: error });
  }
  return {
    playback,
    failure: setupFailure,
    hasPlayRequest: () => requestedPlay,
    dispose() {
      // Fence synchronously, including calls already waiting for native seek/audio mode.
      if (releaseTask) return releaseTask;
      if (released) return Promise.resolve();
      live = false;
      subscription?.remove();
      subscription = null;
      disconnect();
      const task = (async () => {
        while (pending.size) await Promise.allSettled(pending);
        player.release();
        released = true;
      })();
      releaseTask = task;
      void task
        .finally(() => {
          if (releaseTask === task) releaseTask = null;
        })
        .catch(() => {});
      return task;
    },
  };
}
