import { useCallback, useLayoutEffect, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import type { ClientState, ClientStore } from '../core/store';
import type { AgentRequestDto } from '../core/generated/AgentRequestDto';
import { documentScope } from '../documents/document-authority.ts';

function owner(store: ClientStore, state = store.getSnapshot()): string {
  const scope = !store.isSessionRetiring && documentScope(state, store.sessionEpoch);
  return scope ? JSON.stringify([scope, state.snapshot?.account?.relay_url ?? null]) : '';
}
type Observation = Readonly<{
  active: boolean;
  activation: number;
  review: string;
}>;
type Ticket = Readonly<{ lease: DecisionLease; request: string; observation: Observation }>;

/** Only a presentation lease. Requests, decisions and receipts remain in Core. */
class DecisionLease {
  private state: Observation = { active: false, activation: 0, review: '' };
  private listeners = new Set<() => void>();
  constructor(
    private store: ClientStore,
    private scope: string,
    private request: string,
  ) {}
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private update(next: Observation) {
    this.state = next;
    for (const listener of this.listeners) listener();
  }
  observe(review: string) {
    if (review !== this.state.review) this.update({ ...this.state, review });
  }
  activate() {
    this.update({
      ...this.state,
      active: !!this.scope && owner(this.store) === this.scope,
      activation: this.state.activation + 1,
    });
  }
  retire = () =>
    this.update({ ...this.state, active: false, activation: this.state.activation + 1 });
  ticket(observation: Observation): Ticket | null {
    return observation === this.state && this.state.active && owner(this.store) === this.scope
      ? { lease: this, request: this.request, observation }
      : null;
  }
  accepts(ticket: Ticket | null): boolean {
    return this.continues(ticket) && ticket?.observation === this.state;
  }
  continues(ticket: Ticket | null): boolean {
    // An admitted decision publishes canonical changes before its Promise resolves.
    // Those changes retire admission, not this activation's exact result.
    return (
      !!ticket &&
      ticket.lease === this &&
      ticket.request === this.request &&
      this.state.active &&
      ticket.observation.activation === this.state.activation &&
      owner(this.store) === this.scope
    );
  }
}

export function useRequestDecisionLease(
  store: ClientStore,
  state: ClientState,
  request: AgentRequestDto,
  disabled: boolean,
  resetOutput: () => void,
) {
  const scope = owner(store, state);
  const identity = JSON.stringify([request.id, request.agent.id, request.space_id]);
  const lease = useMemo(() => new DecisionLease(store, scope, identity), [store, scope, identity]);
  const observation = useSyncExternalStore(lease.subscribe, lease.getSnapshot, lease.getSnapshot);
  const review = JSON.stringify([
    request.status,
    request.can_always_approve,
    request.agent.is_mine,
    request.agent.owner_id,
    request.tool_approval?.content_hash,
    request.tool_approval?.expires_ms,
    disabled,
  ]);
  useLayoutEffect(() => lease.observe(review), [lease, review]);
  useLayoutEffect(() => () => lease.retire(), [lease]);
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const retire = () => {
        lease.retire();
        resetOutput();
      };
      const activate = () => {
        resetOutput();
        if (foreground) lease.activate();
        else lease.retire();
      };
      activate();
      const session = store.subscribe(() => {
        if (owner(store) !== scope) retire();
      });
      const cleanup = store.registerSessionCleanup(async () => retire());
      const app = AppState.addEventListener('change', (next) => {
        const active = next === 'active';
        if (foreground === active) return;
        foreground = active;
        if (active) activate();
        else retire();
      });
      return () => {
        app.remove();
        session();
        cleanup();
        retire();
      };
    }, [store, lease, scope, resetOutput]),
  );
  return { lease, ticket: lease.ticket(observation), available: !!scope };
}
