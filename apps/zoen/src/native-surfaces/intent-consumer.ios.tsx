import * as AppIntents from 'expo-app-intents';
import { router } from 'expo-router';
import { useClient } from '../core/provider';
import { openSurfaceInvocation } from './intents';
import { surfacesAvailable } from './platform';

function ReadyConsumer() {
  const { store } = useClient();
  AppIntents.useAppIntents(async (pending) => {
    for (const invocation of pending) {
      try {
        const result = await openSurfaceInvocation(invocation, {
          currentOwner: () => {
            const state = store.getSnapshot();
            const account = state.snapshot?.account;
            return state.phase === 'ready' && state.mode === 'account' && account?.unlocked
              ? { identityId: account.identity_id, deviceId: account.device_id }
              : null;
          },
          item: (id) => store.driver.request({ op: 'item', item_id: id }),
          canOpenSpace: async (id) =>
            !!store.getSnapshot().snapshot?.conversations.some((space) => space.id === id),
          open: (path) => router.push(path as '/'),
        });
        // Navigation is safe to replay. Transient reads and another account remain pending.
        // Known malformed/expired and stale-version requests are retired without claiming success.
        if (['opened', 'invalid', 'stale'].includes(result.status))
          await AppIntents.removePendingInvocationAsync(invocation.id);
      } catch {
        /* A failed removal or read stays pending for the next mount/event. */
      }
    }
  });
  return null;
}
export function NativeIntentConsumer() {
  const { phase, mode, snapshot } = useClient();
  const account = snapshot?.account;
  return surfacesAvailable() && phase === 'ready' && mode === 'account' && account?.unlocked ? (
    <ReadyConsumer key={`${account.identity_id}:${account.device_id}`} />
  ) : null;
}
