import { createContext, use, useEffect, useState, type ReactNode } from 'react';
import { AppState } from 'react-native';
import { useClient } from '../core/provider';
import { SurfaceCoordinator } from './coordinator';
import { createSurfacePort } from './platform';
import { surfaceOwner, surfaceSpaces } from './access';

const Context = createContext<SurfaceCoordinator | null>(null);
const InitializationContext = createContext(false);
export function useNativeSurfaces() {
  return use(Context);
}
export function useNativeSurfacesInitialized() {
  return use(InitializationContext);
}

/** App-only coordinator: extensions receive compact projections, never an engine or vault. */
export function NativeSurfacesProvider({ children }: { children: ReactNode }) {
  const { store } = useClient();
  const [coordinator, setCoordinator] = useState<SurfaceCoordinator | null>(null);
  const [initialized, setInitialized] = useState(false);
  useEffect(() => {
    let disposed = false;
    let active: SurfaceCoordinator | null = null;
    let unregisterCleanup: (() => void) | undefined;
    const owner = () => surfaceOwner(store.getSnapshot());
    const unsubscribe = store.subscribe(() => {
      void active?.refresh();
    });
    const foreground = AppState.addEventListener('change', (state) => {
      if (state === 'active') void active?.refresh();
    });
    void createSurfacePort()
      .then((port) => {
        if (disposed) return;
        if (!port) {
          setInitialized(true);
          return;
        }
        active = new SurfaceCoordinator(
          {
            loading: () => store.getSnapshot().phase === 'opening',
            owner,
            spaces: () => surfaceSpaces(store.getSnapshot()),
            item: (id) => store.driver.request({ op: 'item', item_id: id }),
            items: (ids, currentOwner) =>
              store.driver.request({
                op: 'native_surface_items',
                expected_identity_id: currentOwner.identityId,
                expected_device_id: currentOwner.deviceId,
                item_ids: [...ids],
              }),
          },
          port,
        );
        const current = active;
        unregisterCleanup = store.registerDeviceCleanup(() => current.eraseDevice());
        active.activate();
        setCoordinator(active);
        setInitialized(true);
      })
      .catch(() => {
        /* Unsupported development binaries do not expose these controls. */
        if (!disposed) setInitialized(true);
      });
    return () => {
      disposed = true;
      unsubscribe();
      unregisterCleanup?.();
      foreground.remove();
      active?.dispose();
    };
  }, [store]);
  return (
    <InitializationContext value={initialized}>
      <Context value={coordinator}>{children}</Context>
    </InitializationContext>
  );
}
