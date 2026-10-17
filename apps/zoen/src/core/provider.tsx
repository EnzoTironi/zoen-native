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

const Context = createContext<ClientStore | null>(null);
export function ClientProvider({ children }: { children: ReactNode }) {
  const [store] = useState(() => new ClientStore(createDriver()));
  useEffect(() => {
    void store.open();
    return () => {
      void store.close();
    };
  }, [store]);
  return <Context value={store}>{children}</Context>;
}
export function useClient() {
  const store = use(Context);
  if (!store) throw new Error('ClientProvider is missing.');
  return { store, ...useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot) };
}
