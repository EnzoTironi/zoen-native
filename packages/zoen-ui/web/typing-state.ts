import { array, object, publicKey, string } from './boundary.ts';

/** Transient UI events emitted by Core after the authenticated relay/roster checks. */
export type CoreUiEvent =
  | { kind: 'typing'; space: string; from: string; name: string | null }
  | { kind: 'stopped'; space: string; from: string }
  | { kind: 'space_changed'; space: string };
export interface TypingPeer { identity: string; name: string | null; expiresAt: number }
export type TypingState = Readonly<Record<string, readonly TypingPeer[]>>;
// SyncModel.ephemeral uses six seconds, independently of the display/fixture clock.
export const TYPING_TTL_MS = 6_000;

export function parseCoreUiEvents(input: unknown): CoreUiEvent[] {
  if (input === undefined) return []; // Older saved clients have no transient events.
  return array(input).map(input => {
    const value = object(input);
    const space = string(value.space);
    if (!space) throw new Error('Invalid typing space.');
    if (value.kind === 'space_changed') return { kind: 'space_changed', space };
    const from = publicKey(value.from);
    if (value.kind === 'stopped') return { kind: 'stopped', space, from };
    if (value.kind !== 'typing') throw new Error('Invalid transient UI event.');
    return { kind: 'typing', space, from, name: value.name === null ? null : string(value.name) };
  });
}

export function typingPeers(state: TypingState, space: string): readonly TypingPeer[] {
  return Object.hasOwn(state, space) ? state[space]! : [];
}

export function expireTyping(state: TypingState, now: number, spaces: ReadonlySet<string>): TypingState {
  let changed = false;
  const entries = Object.entries(state).flatMap(([space, peers]) => {
    const active = spaces.has(space) ? peers.filter(peer => peer.expiresAt > now) : [];
    if (active.length !== peers.length) changed = true;
    return active.length ? [[space, active] as const] : [];
  });
  return changed ? Object.fromEntries(entries) : state;
}

export function reduceTyping(state: TypingState, events: readonly CoreUiEvent[], now: number,
  identity: string, spaces: ReadonlySet<string>): TypingState {
  let next = expireTyping(state, now, spaces);
  for (const event of events) {
    if (!spaces.has(event.space)) continue;
    const previous = typingPeers(next, event.space);
    if (event.kind !== 'space_changed' && event.from === identity) continue;
    const peers = event.kind === 'space_changed' ? [] : event.kind === 'stopped'
      ? previous.filter(peer => peer.identity !== event.from)
      : [...previous.filter(peer => peer.identity !== event.from),
          { identity: event.from, name: event.name, expiresAt: now + TYPING_TTL_MS }];
    if (!peers.length && !previous.length) continue;
    if (peers.length) next = { ...next, [event.space]: peers };
    else { const { [event.space]: removed, ...rest } = next; void removed; next = rest; }
  }
  return next;
}
