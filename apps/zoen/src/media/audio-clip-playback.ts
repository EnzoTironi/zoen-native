import { setAudioModeAsync, useAudioPlayer } from 'expo-audio';
import { useLayoutEffect, useMemo } from 'react';
import { AudioClipController, type AudioClipPlayback } from './audio-clip-controller.ts';
import { mediaPlayback } from './playback-owner.ts';
export type { AudioClipPlayback, AudioClipPlaybackState } from './audio-clip-controller.ts';

const alwaysCurrent = () => true;
/** One source-owned Expo hook, shared by any number of controls-only presentations. */
export function useAudioClipPlayback({
  uri,
  durationMs = 0,
  enabled = true,
  isCurrent = alwaysCurrent,
}: {
  uri: string | null;
  durationMs?: number;
  enabled?: boolean;
  isCurrent?: () => boolean;
}): AudioClipPlayback {
  const player = useAudioPlayer(uri ? { uri } : null, {
    updateInterval: 100,
    allowsExternalPlayback: false,
  });
  const playback = useMemo(() => new AudioClipController(0, mediaPlayback, player), [player]);
  useLayoutEffect(() => {
    const disconnect = playback.connect({
      playing: () => player.playing,
      currentSeconds: () => player.currentTime,
      play: () => player.play(),
      pause: () => player.pause(),
      seek: (seconds) => player.seekTo(seconds, 0, 0),
      setRate: (rate) => player.setPlaybackRate(rate),
      reload: () => {
        if (uri) player.replace({ uri });
      },
      preparePlayback: () =>
        setAudioModeAsync({
          playsInSilentMode: true,
          allowsRecording: false,
          shouldPlayInBackground: false,
          interruptionMode: 'doNotMix',
        }),
    });
    let live = true;
    const playerId = player.id;
    // The SDK status hook retains its previous source's initial state. Observe
    // this exact SharedObject instead; queued old events cannot reach its successor.
    const subscription = player.addListener('playbackStatusUpdate', (next) => {
      if (live && next.id === playerId) playback.observe(next);
    });
    // SDK 58's official status hook seeds from this same typed snapshot. Read it
    // here so a failure preceding subscription still exposes Retry on this source.
    playback.observe(player.currentStatus);
    return () => {
      live = false;
      subscription.remove();
      disconnect();
    };
  }, [playback, player, uri]);
  useLayoutEffect(() => {
    playback.configure(enabled && Boolean(uri), isCurrent, durationMs);
  }, [playback, enabled, uri, isCurrent, durationMs]);
  return playback;
}
