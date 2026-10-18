import { useCallback, useSyncExternalStore } from 'react';
import type { ClientStore } from '../core/store';
import type { UndoToken } from '../core/generated/UndoToken';

// Transient action affordance only. Documents and their versions remain in Rust.
const actions = new WeakMap<
  ClientStore,
  { tokens: Map<string, UndoToken>; listeners: Set<() => void> }
>();
function state(store: ClientStore) {
  let value = actions.get(store);
  if (!value) {
    value = { tokens: new Map(), listeners: new Set() };
    actions.set(store, value);
  }
  return value;
}
function key(store: ClientStore, item: string) {
  return `${store.getSnapshot().snapshot?.account?.identity_id ?? ''}:${item}`;
}
export function rememberPlanUndo(store: ClientStore, token: UndoToken) {
  const value = state(store);
  value.tokens.set(key(store, token.item_id), token);
  for (const listener of value.listeners) listener();
}
export function clearPlanUndo(store: ClientStore, item: string) {
  const value = state(store);
  value.tokens.delete(key(store, item));
  for (const listener of value.listeners) listener();
}
export function usePlanUndo(store: ClientStore, item: string): UndoToken | null {
  const subscribe = useCallback(
    (listener: () => void) => {
      const value = state(store);
      value.listeners.add(listener);
      return () => {
        value.listeners.delete(listener);
      };
    },
    [store],
  );
  const snapshot = useCallback(
    () => state(store).tokens.get(key(store, item)) ?? null,
    [store, item],
  );
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}
