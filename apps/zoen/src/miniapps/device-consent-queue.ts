export type ConsentChoice = 'once' | 'always' | 'deny';
export type ConsentRequest = Readonly<{ id: number; capability: string; purpose: string }>;
export type DeviceConsentSnapshot = Readonly<{
  current: ConsentRequest | null;
  phase: 'idle' | 'presented' | 'dismissing';
}>;

type Pending = {
  request: ConsentRequest;
  choice?: ConsentChoice;
  resolve: ((choice: ConsentChoice) => void) | null;
};
const idle: DeviceConsentSnapshot = Object.freeze({ current: null, phase: 'idle' });
// A native callback from a retired owner must never match a later owner's sheet.
let nextRequestId = 0;

export interface DeviceConsentQueue {
  ask(capability: string, purpose: string): Promise<ConsentChoice>;
  choose(id: number, choice: ConsentChoice): boolean;
  dismissed(id: number): boolean;
  cancelAll(): void;
  dispose(): void;
  getSnapshot(): DeviceConsentSnapshot;
  subscribe(listener: () => void): () => void;
}

/** A choice is staged until UIKit confirms dismissal of that exact presentation. */
export function createDeviceConsentQueue(): DeviceConsentQueue {
  let state = idle;
  let current: Pending | null = null;
  let pending: Pending[] = [];
  let disposed = false;
  const listeners = new Set<() => void>();

  function publish(next: DeviceConsentSnapshot) {
    if (state.current === next.current && state.phase === next.phase) return;
    state = Object.freeze(next);
    for (const listener of listeners) listener();
  }
  function settle(entry: Pending, choice: ConsentChoice) {
    const resolve = entry.resolve;
    entry.resolve = null;
    resolve?.(choice);
  }
  function advance() {
    current = pending.shift() ?? null;
    publish(current ? { current: current.request, phase: 'presented' } : idle);
  }
  function denyPending() {
    const queued = pending;
    pending = [];
    for (const entry of queued) settle(entry, 'deny');
  }

  return {
    ask(capability, purpose) {
      if (disposed || !Number.isSafeInteger(nextRequestId + 1)) return Promise.resolve('deny');
      const request = Object.freeze({ id: ++nextRequestId, capability, purpose });
      const result = new Promise<ConsentChoice>((resolve) => {
        pending.push({ request, resolve });
      });
      if (!current) advance();
      return result;
    },
    choose(id, choice) {
      if (
        disposed ||
        !current ||
        current.request.id !== id ||
        state.phase !== 'presented' ||
        !['once', 'always', 'deny'].includes(choice)
      )
        return false;
      current.choice = choice;
      publish({ current: current.request, phase: 'dismissing' });
      return true;
    },
    dismissed(id) {
      if (disposed || !current || current.request.id !== id) return false;
      const finished = current;
      current = null;
      settle(finished, finished.choice ?? 'deny');
      advance();
      return true;
    },
    cancelAll() {
      if (disposed) return;
      denyPending();
      if (!current) return;
      settle(current, 'deny');
      // Retain the presentation barrier after synchronous revocation. A foreground
      // request may queue, but cannot appear under a sheet that is still closing.
      current.choice = 'deny';
      publish({ current: current.request, phase: 'dismissing' });
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      denyPending();
      if (current) settle(current, 'deny');
      current = null;
      publish(idle);
      listeners.clear();
    },
    getSnapshot: () => state,
    subscribe(listener) {
      if (disposed) return () => {};
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
