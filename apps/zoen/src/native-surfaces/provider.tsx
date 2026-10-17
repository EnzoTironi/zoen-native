import { createContext, use, useEffect, useState, type ReactNode } from 'react';
import { AppState } from 'react-native';
import { useClient } from '../core/provider';
import { SurfaceCoordinator } from './coordinator';
import { createSurfacePort } from './platform';
import type { SurfaceOwner } from './snapshot';

const Context = createContext<SurfaceCoordinator | null>(null);
export function useNativeSurfaces() {
  return use(Context);
}

/** App-only coordinator: extensions receive compact projections, never an engine or vault. */
export function NativeSurfacesProvider({ children }: { children: ReactNode }) {
  const { store } = useClient();
  const [coordinator, setCoordinator] = useState<SurfaceCoordinator | null>(null);
  useEffect(() => {
    let disposed = false;
    let active: SurfaceCoordinator | null = null;
    let unregisterCleanup: (() => void) | undefined;
    const owner = (): SurfaceOwner | null => {
      const state = store.getSnapshot();
      const account = state.snapshot?.account;
      return state.mode === 'account' &&
        state.phase === 'ready' &&
        account?.unlocked &&
        !account.setup_pending
        ? { identityId: account.identity_id, deviceId: account.device_id }
        : null;
    };
    const unsubscribe = store.subscribe(() => {
      void active?.refresh();
    });
    const foreground = AppState.addEventListener('change', (state) => {
      if (state === 'active') void active?.refresh();
    });
    void createSurfacePort()
      .then((port) => {
        if (!port || disposed) return;
        active = new SurfaceCoordinator(
          {
            loading: () => store.getSnapshot().phase === 'opening',
            owner,
            spaces: () =>
              new Map(
                store
                  .getSnapshot()
                  .snapshot?.conversations.map((space) => [space.id, space.title]) ?? [],
              ),
            item: (id) => store.driver.request({ op: 'item', item_id: id }),
          },
          port,
        );
        const current = active;
        unregisterCleanup = store.registerDeviceCleanup(() => current.eraseDevice());
        active.activate();
        setCoordinator(active);
      })
      .catch(() => {
        /* Unsupported development binaries do not expose these controls. */
      });
    return () => {
      disposed = true;
      unsubscribe();
      unregisterCleanup?.();
      foreground.remove();
      active?.dispose();
    };
  }, [store]);
  return <Context value={coordinator}>{children}</Context>;
}
