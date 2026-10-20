import {
  createContext,
  use,
  useEffect,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { createDriver } from './driver';
import { ClientStore } from './store';
import { attachDeviceCacheCleanup } from '../media/device-cache';
import { MessageAudioProvider } from '../media/message-audio-provider';

const Context = createContext<ClientStore | null>(null);
export function ClientProvider({ children }: { children: ReactNode }) {
  const [store] = useState(() => new ClientStore(createDriver()));
  useEffect(() => {
    const detachCacheCleanup = attachDeviceCacheCleanup(store);
    void store.open();
    return () => {
      detachCacheCleanup();
      void store.close({ force: true }).catch(() => {});
    };
  }, [store]);
  return (
    <Context value={store}>
      <MessageAudioProvider store={store}>{children}</MessageAudioProvider>
    </Context>
  );
}
export function useClient() {
  const store = use(Context);
  if (!store) throw new Error('ClientProvider is missing.');
  return { store, ...useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot) };
}
