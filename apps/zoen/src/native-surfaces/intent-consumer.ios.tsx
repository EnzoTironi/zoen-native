import * as AppIntents from 'expo-app-intents';
import { router } from 'expo-router';
import { useClient } from '../core/provider';
import { openSurfaceInvocation } from './intents';
import { surfacesAvailable } from './platform';
import { surfaceOwner, surfaceSpaces } from './access';

function ReadyConsumer() {
  const { store } = useClient();
  AppIntents.useAppIntents(async (pending) => {
    for (const invocation of pending) {
      try {
        const result = await openSurfaceInvocation(invocation, {
          currentOwner: () => surfaceOwner(store.getSnapshot()),
          item: (id) => store.driver.request({ op: 'item', item_id: id }),
          canOpenSpace: async (id) => surfaceSpaces(store.getSnapshot()).has(id),
          open: (path) => {
            store.assertExternalNavigationAllowed();
            router.push(path as '/');
          },
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
  const state = useClient();
  const owner = surfaceOwner(state);
  return surfacesAvailable() && owner ? (
    <ReadyConsumer key={`${owner.identityId}:${owner.deviceId}`} />
  ) : null;
}
