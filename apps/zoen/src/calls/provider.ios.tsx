import { setAudioModeAsync, setIsAudioActiveAsync } from 'expo-audio';
import { createContext, use, useEffect, useMemo, type ReactNode } from 'react';
import Native from '../../modules/zoen-core/src/ZoenCoreModule';
import { useClient } from '../core/provider';
import { callAudioFocus } from '../media/call-audio-focus.ts';
import { nativeCaptureOwner } from '../media/native-capture-owner.ts';
import { mediaPlayback } from '../media/playback-owner.ts';

type Calls = { available: boolean; start(space: string, video: boolean): Promise<void> };
const Context = createContext<Calls>({ available: false, start: async () => {} });

export function NativeCallsProvider({ children }: { children: ReactNode }) {
  const { store, snapshot, mode } = useClient();
  const identity = snapshot?.account?.identity_id;
  const device = snapshot?.account?.device_id;
  const available =
    mode === 'account' &&
    snapshot?.account?.unlocked === true &&
    typeof Native?.startCall === 'function' &&
    typeof Native?.callAudioReply === 'function';
  useEffect(() => {
    if (!Native?.startCall || !Native.callAudioReply) return;
    const native = Native;
    let tail = Promise.resolve();
    const subscription = native.addListener('onCallAudio', ({ json }) => {
      if (json.length > 512) return;
      let value: unknown;
      try {
        value = JSON.parse(json);
      } catch {
        return;
      }
      if (!value || typeof value !== 'object' || !('ticket' in value) || !('action' in value))
        return;
      const { ticket, action } = value;
      if (
        typeof ticket !== 'string' ||
        !/^[a-f0-9-]{36}$/i.test(ticket) ||
        (action !== 'acquire' && action !== 'release')
      )
        return;
      tail = tail
        .then(async () => {
          if (action === 'acquire') {
            try {
              // Fence synchronously before awaiting the recorder or an old audio mode.
              callAudioFocus.begin(ticket);
              mediaPlayback.pauseAll();
              await nativeCaptureOwner.stop();
              await callAudioFocus.drain();
              await setAudioModeAsync({
                allowsRecording: false,
                shouldPlayInBackground: false,
                playsInSilentMode: true,
                interruptionMode: 'doNotMix',
              });
              await setIsAudioActiveAsync(false);
              await native.callAudioReply!(ticket, true);
            } catch {
              await native.callAudioReply!(ticket, false);
            }
          } else if (callAudioFocus.owns(ticket)) {
            // Native capture has stopped and CallKit has deactivated this exact call.
            // Players remain paused; a fresh explicit media action configures audio.
            mediaPlayback.pauseAll();
            callAudioFocus.end(ticket);
          }
        })
        .catch(() => {});
    });
    return () => {
      subscription.remove();
    };
  }, []);
  const value = useMemo<Calls>(
    () => ({
      available,
      async start(space, video) {
        const now = store.getSnapshot();
        if (
          !available ||
          now.mode !== 'account' ||
          now.snapshot?.account?.identity_id !== identity ||
          now.snapshot?.account?.device_id !== device
        )
          throw new Error('The call account changed.');
        await Native!.startCall!(space, video);
      },
    }),
    [available, store, identity, device],
  );
  return <Context value={value}>{children}</Context>;
}
export function useNativeCalls(): Calls {
  return use(Context);
}
