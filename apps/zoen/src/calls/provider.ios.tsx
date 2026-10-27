import { setAudioModeAsync, setIsAudioActiveAsync } from 'expo-audio';
import {
  createContext,
  use,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
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
  const { store, snapshot, mode, phase } = useClient();
  // Reopening can retain identity/device IDs; UI callbacks still belong to the original realm.
  const sessionEpoch = store.sessionEpoch;
  const mounted = useRef(true);
  useLayoutEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const identity = snapshot?.account?.identity_id;
  const device = snapshot?.account?.device_id;
  const [presentation, setPresentation] = useState<{
    epoch: number;
    call: CallPresentation;
  } | null>(null);
  const currentOwner = useCallback(() => {
    const current = store.getSnapshot();
    const account = current.snapshot?.account;
    return mounted.current &&
      !store.isSessionRetiring &&
      store.sessionEpoch === sessionEpoch &&
      current.phase === 'ready' &&
      current.mode === 'account' &&
      account?.unlocked === true &&
      !account.setup_pending &&
      account.identity_id === identity &&
      account.device_id === device
      ? account
      : null;
  }, [store, sessionEpoch, identity, device]);
  const ownerReady =
    phase === 'ready' &&
    !store.isSessionRetiring &&
    mode === 'account' &&
    snapshot?.account?.unlocked === true &&
    !snapshot.account.setup_pending;
  const activeCall =
    ownerReady &&
    presentation?.epoch === sessionEpoch &&
    presentation.call.identity_id === identity &&
    presentation.call.device_id === device
      ? presentation.call
      : null;
  const available =
    ownerReady &&
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
        const account = currentOwner();
        return { identity: account?.identity_id, device: account?.device_id };
      },
      changed: (call) => setPresentation(call ? { epoch: sessionEpoch, call } : null),
    });
  }, [identity, device, store, sessionEpoch, currentOwner]);
  const value = useMemo<Calls>(
    () => ({
      available,
      activeCall,
      async showActiveCall() {
        if (!activeCall || !currentOwner() || !Native?.showCurrentCall) return;
        await Native.showCurrentCall(
          activeCall.identity_id,
          activeCall.device_id,
          activeCall.call_id,
        );
      },
      async start(space, video) {
        if (!currentOwner()) return;
        if (!available) throw new Error('The call account changed.');
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
    [available, activeCall, currentOwner],
  );
  return <Context value={value}>{children}</Context>;
}
export function useNativeCalls(): Calls {
  return use(Context);
}
