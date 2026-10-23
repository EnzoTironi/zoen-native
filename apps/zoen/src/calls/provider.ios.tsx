import { setAudioModeAsync, setIsAudioActiveAsync } from 'expo-audio';
import { createContext, use, useEffect, useMemo, useState, type ReactNode } from 'react';
import Native from '../../modules/zoen-core/src/ZoenCoreModule';
import { useClient } from '../core/provider';
import { callAudioFocus } from '../media/call-audio-focus.ts';
import { nativeCaptureOwner } from '../media/native-capture-owner.ts';
import { mediaPlayback } from '../media/playback-owner.ts';
import { createCallAudioHandoff } from './audio-handoff.ts';
import { watchCallPresentation, type CallPresentation } from './presentation.ts';

type Calls = {
  available: boolean;
  activeCall: CallPresentation | null;
  showActiveCall(): Promise<void>;
  start(space: string, video: boolean): Promise<void>;
};
const Context = createContext<Calls>({
  available: false,
  activeCall: null,
  showActiveCall: async () => {},
  start: async () => {},
});

export function NativeCallsProvider({ children }: { children: ReactNode }) {
  const { store, snapshot, mode } = useClient();
  const identity = snapshot?.account?.identity_id;
  const device = snapshot?.account?.device_id;
  const [presentation, setPresentation] = useState<CallPresentation | null>(null);
  const activeCall =
    presentation?.identity_id === identity && presentation?.device_id === device
      ? presentation
      : null;
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
  useEffect(() => {
    if (!Native?.callPresentation || !Native.showCurrentCall || !identity || !device) return;
    const native = Native;
    return watchCallPresentation({
      listen(changed) {
        const subscription = native.addListener('onCallState', ({ json }) => changed(json));
        return () => subscription.remove();
      },
      current: () => native.callPresentation!(),
      owner() {
        const current = store.getSnapshot();
        return {
          identity: current.mode === 'account' ? current.snapshot?.account?.identity_id : undefined,
          device: current.snapshot?.account?.device_id,
        };
      },
      changed: setPresentation,
    });
  }, [identity, device, store]);
  const value = useMemo<Calls>(
    () => ({
      available,
      activeCall,
      async showActiveCall() {
        const current = store.getSnapshot();
        if (
          !activeCall ||
          current.mode !== 'account' ||
          current.snapshot?.account?.identity_id !== activeCall.identity_id ||
          current.snapshot?.account?.device_id !== activeCall.device_id ||
          !Native?.showCurrentCall
        )
          return;
        await Native.showCurrentCall(
          activeCall.identity_id,
          activeCall.device_id,
          activeCall.call_id,
        );
      },
      async start(space, video) {
        const now = store.getSnapshot();
        if (
          !available ||
          now.mode !== 'account' ||
          now.snapshot?.account?.identity_id !== identity ||
          now.snapshot?.account?.device_id !== device
        )
          throw new Error('The call account changed.');
        if (activeCall?.space_id === space && Native?.showCurrentCall) {
          await Native.showCurrentCall(
            activeCall.identity_id,
            activeCall.device_id,
            activeCall.call_id,
          );
          return;
        }
        await Native!.startCall!(space, video);
      },
    }),
    [available, activeCall, store, identity, device],
  );
  return <Context value={value}>{children}</Context>;
}
export function useNativeCalls(): Calls {
  return use(Context);
}
