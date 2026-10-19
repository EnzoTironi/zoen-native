import { useCallback, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { createDeviceConsentQueue, type ConsentChoice } from './device-consent-queue';

/** One view's consent. A new foreground request never revives a retired decision. */
export function useDeviceConsent() {
  const [queue] = useState(createDeviceConsentQueue);
  const mounted = useRef(true);
  const active = useRef(AppState.currentState === 'active');
  const snapshot = useSyncExternalStore(queue.subscribe, queue.getSnapshot, queue.getSnapshot);
  useLayoutEffect(() => {
    mounted.current = true;
    active.current = AppState.currentState === 'active';
    const subscription = AppState.addEventListener('change', (state) => {
      active.current = state === 'active';
      if (state === 'background') queue.cancelAll();
    });
    return () => {
      mounted.current = false;
      active.current = false;
      // Reusable cancellation also survives React's development effect replay.
      // The host separately closes frame authority before admitting another RPC.
      queue.cancelAll();
      subscription.remove();
    };
  }, [queue]);
  const ask = useCallback(
    (capability: string, purpose: string) =>
      mounted.current && active.current
        ? queue.ask(capability, purpose)
        : Promise.resolve<ConsentChoice>('deny'),
    [queue],
  );
  return {
    ...snapshot,
    ask,
    choose: queue.choose,
    dismissed: queue.dismissed,
    cancelAll: queue.cancelAll,
  };
}
