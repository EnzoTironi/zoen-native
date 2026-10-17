import { useEffect, useState } from 'react';
import { router, useLocalSearchParams } from 'expo-router';
import { EmptyState, Loading, NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { openSurfaceInvocation } from '../native-surfaces/intents';
import { surfaceRouteInvocation } from '../native-surfaces/links';

export default function SurfaceEntry() {
  const params = useLocalSearchParams();
  const encoded = JSON.stringify(params);
  const { store, phase, mode } = useClient();
  const [failure, setFailure] = useState(false);
  useEffect(() => {
    if (phase !== 'ready') return;
    let active = true;
    void openSurfaceInvocation(surfaceRouteInvocation(JSON.parse(encoded)), {
      currentOwner: () => {
        const state = store.getSnapshot();
        const account = state.snapshot?.account;
        return active && state.mode === 'account' && state.phase === 'ready' && account?.unlocked
          ? { identityId: account.identity_id, deviceId: account.device_id }
          : null;
      },
      item: (id) => store.driver.request({ op: 'item', item_id: id }),
      canOpenSpace: async (id) =>
        !!store.getSnapshot().snapshot?.conversations.some((space) => space.id === id),
      open: (path) => router.replace(path as '/'),
    }).then((result) => {
      if (active && result.status !== 'opened') setFailure(true);
    });
    return () => {
      active = false;
    };
  }, [encoded, store, phase, mode]);
  return failure ? (
    <EmptyState
      title="Abra este miniapp no Zoen"
      detail="O atalho mudou ou não está disponível nesta conta."
    >
      <NativeButton label="Voltar ao Zoen" onPress={() => router.replace('/')} />
    </EmptyState>
  ) : (
    <Loading />
  );
}
