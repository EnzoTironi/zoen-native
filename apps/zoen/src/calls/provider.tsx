import type { ReactNode } from 'react';
import type { CallPresentation } from './presentation.ts';

/** Native calls are delivered first on iOS; other platforms expose no inert controls. */
export function NativeCallsProvider({ children }: { children: ReactNode }) {
  return children;
}
export function useNativeCalls() {
  return {
    available: false,
    activeCall: null as CallPresentation | null,
    showActiveCall: async () => {},
    start: async (_space: string, _video: boolean) => {},
  };
}
