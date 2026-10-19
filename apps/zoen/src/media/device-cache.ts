import { Image } from 'expo-image';
import type { ClientStore } from '../core/store';
import { sensitiveCacheWork } from './cache-work';

/** The retained device-cleanup hook drains SDK writers before the native namespace sweep. */
export function attachDeviceCacheCleanup(store: ClientStore): () => void {
  if (process.env.EXPO_OS !== 'ios' && process.env.EXPO_OS !== 'android') return () => {};
  const synchronize = () => {
    const state = store.getSnapshot();
    if (!state.erasePending && (state.phase === 'ready' || state.phase === 'onboarding'))
      sensitiveCacheWork.resume();
  };
  const unsubscribe = store.subscribe(synchronize);
  const unregister = store.registerDeviceCleanup(() =>
    sensitiveCacheWork.erase(async () => {
      const results = await Promise.allSettled([
        Promise.resolve().then(() => Image.clearMemoryCache()),
        Promise.resolve().then(() => Image.clearDiskCache()),
      ]);
      if (results.some((result) => result.status === 'rejected' || result.value !== true))
        throw new Error('Não foi possível apagar as imagens deste aparelho. Tente novamente.');
    }),
  );
  synchronize();
  return () => {
    unregister();
    unsubscribe();
  };
}
