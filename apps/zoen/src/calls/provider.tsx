import type { ReactNode } from 'react';

/** Native calls are delivered first on iOS; other platforms expose no inert controls. */
export function NativeCallsProvider({ children }: { children: ReactNode }) {
  return children;
}
export function useNativeCalls() {
  return { available: false, start: async (_space: string, _video: boolean) => {} };
}
