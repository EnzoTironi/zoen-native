import { t } from '../i18n/index.ts';
import { useEffect, useState, useSyncExternalStore } from 'react';
import { router, useLocalSearchParams } from 'expo-router';
import { EmptyState, Loading, NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { openSurfaceInvocation } from '../native-surfaces/intents';
import { surfaceRouteInvocation } from '../native-surfaces/links';
import { useNativeSurfaces, useNativeSurfacesInitialized } from '../native-surfaces/provider';
import { surfaceOwner, surfaceSpaces } from '../native-surfaces/access';

const noSubscription = () => () => {};
const noSurfaces = () => null;

export default function SurfaceEntry() {
  const params = useLocalSearchParams();
  const encoded = JSON.stringify(params);
  const { store, phase, mode } = useClient();
  const surfaces = useNativeSurfaces();
  const initialized = useNativeSurfacesInitialized();
  const surfaceState = useSyncExternalStore(
    surfaces?.subscribe ?? noSubscription,
    surfaces?.getSnapshot ?? noSurfaces,
    surfaces?.getSnapshot ?? noSurfaces,
  );
  const [failure, setFailure] = useState(false);
  useEffect(() => {
    if (phase !== 'ready') return;
    let active = true;
    const invocation = surfaceRouteInvocation(JSON.parse(encoded));
    const currentOwner = () => (active ? surfaceOwner(store.getSnapshot()) : null);
    // Cold-start URLs can arrive before the native preference file is restored.
    // Changes to ready/busy re-admit this effect; unsupported ports fail closed.
    if (
      invocation.name === 'zoen.openActivity' &&
      (!initialized ||
        (surfaces &&
          currentOwner() &&
          (surfaceState?.busy || (!surfaceState?.ready && !surfaceState?.error))))
    )
      return;
    void openSurfaceInvocation(invocation, {
      currentOwner,
      item: (id) => store.driver.request({ op: 'item', item_id: id }),
      canOpenSpace: async (id) => surfaceSpaces(store.getSnapshot()).has(id),
      canOpenActivity: (item, linkId) => active && !!surfaces?.canOpenActivity(item, linkId),
      open: (path) => router.replace(path as '/'),
    }).then((result) => {
      if (active && result.status !== 'opened') setFailure(true);
    });
    return () => {
      active = false;
    };
  }, [encoded, store, phase, mode, initialized, surfaces, surfaceState]);
  return failure ? (
    <EmptyState
      title={t('chat.openThisMiniappInZoen')}
      detail={t('chat.theShortcutHasChangedOrIsNotAvailable')}
    >
      <NativeButton label={t('chat.backToZoen')} onPress={() => router.replace('/')} />
    </EmptyState>
  ) : (
    <Loading />
  );
}
