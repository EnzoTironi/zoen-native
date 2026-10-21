import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { useClient } from '../core/provider';
import { MessageContextStage } from './message-context-source';
import type { MessageContextStageHandle } from './message-context-ports';

/** Keep the old native Stage mounted until its retirement barrier has drained.
 * Publishing Opening/Erasing cannot replace its ref with another account's Stage.
 * Only a successfully opened session installs a fresh owner and gesture surface. */
export function MessageContextSession({ children }: { children: ReactNode }) {
  const { store, phase, snapshot, mode } = useClient();
  const ref = useRef<MessageContextStageHandle | null>(null);
  const [scope, setScope] = useState('initial');
  const owner =
    phase === 'ready'
      ? JSON.stringify([
          mode,
          snapshot?.account?.identity_id,
          snapshot?.account?.device_id,
          mode === 'demo' ? snapshot?.me?.id : null,
          store.sessionEpoch,
        ])
      : null;
  if (owner !== null && scope !== owner) setScope(owner);
  useLayoutEffect(() => {
    // Capture this exact native instance; a late cleanup must never close the
    // next session's Stage through a mutable ref.
    const stage = ref.current;
    const retire = async () => {
      await stage?.retireContexts();
    };
    const unregisterSession = store.registerSessionCleanup(retire);
    const unregisterDevice = store.registerDeviceCleanup(retire);
    return () => {
      unregisterSession();
      unregisterDevice();
    };
  }, [store, scope]);
  return (
    <MessageContextStage
      key={scope}
      ref={ref}
      active={phase === 'ready' && !store.isSessionRetiring}
    >
      {children}
    </MessageContextStage>
  );
}
