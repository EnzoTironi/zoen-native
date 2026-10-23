import { setAudioModeAsync, setIsAudioActiveAsync } from 'expo-audio';
import { createContext, use, useEffect, useMemo, type ReactNode } from 'react';
import Native from '../../modules/zoen-core/src/ZoenCoreModule';
import { useClient } from '../core/provider';
import { callAudioFocus } from '../media/call-audio-focus.ts';
import { nativeCaptureOwner } from '../media/native-capture-owner.ts';
import { mediaPlayback } from '../media/playback-owner.ts';
import { createCallAudioHandoff } from './audio-handoff.ts';

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
    const handoff = createCallAudioHandoff({
      begin: (ticket) => callAudioFocus.begin(ticket),
      owns: (ticket) => callAudioFocus.owns(ticket),
      end: (ticket) => callAudioFocus.end(ticket),
      pause: () => mediaPlayback.pauseAll(),
      stopCapture: () => nativeCaptureOwner.stop(),
      drain: () => callAudioFocus.drain(),
      async deactivateExpo() {
        await setAudioModeAsync({
          allowsRecording: false,
          shouldPlayInBackground: false,
          playsInSilentMode: true,
          interruptionMode: 'doNotMix',
        });
        await setIsAudioActiveAsync(false);
      },
      reply: (ticket, success) => native.callAudioReply!(ticket, success),
    });
    const subscription = native.addListener('onCallAudio', ({ json }) => {
      void handoff(json);
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
