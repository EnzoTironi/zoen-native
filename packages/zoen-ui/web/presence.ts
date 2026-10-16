/** Relay typing is transient, never persisted with the encrypted conversation log. */
export const TYPING_TTL_MS = 6_000;
export const TYPING_REFRESH_MS = 3_000;
export type PersonPresence = { online: boolean; last_seen_ms: number | null };
export type PresenceSnapshot = { people: Record<string, PersonPresence>; typing: Record<string, string[]>; hideLastSeen: boolean | null };

export class LivePresence {
  private people: Record<string, PersonPresence> = {};
  private typing = new Map<string, Map<string, ReturnType<typeof setTimeout>>>();
  private privacy: boolean | null = null;
  private readonly changed: () => void;
  constructor(changed: () => void) { this.changed = changed; }

  snapshot(): PresenceSnapshot {
    return { people: { ...this.people }, typing: Object.fromEntries([...this.typing].map(([space, people]) => [space, [...people.keys()]])), hideLastSeen: this.privacy };
  }

  presence(identity: string, online: boolean, lastSeen: number | null): void {
    // A hidden/unknown snapshot replaces the old timestamp; it must never linger.
    this.people[identity] = { online, last_seen_ms: online ? null : lastSeen };
    if (!online) for (const [space, people] of this.typing) {
      clearTimeout(people.get(identity)); people.delete(identity);
      if (!people.size) this.typing.delete(space);
    }
    this.changed();
  }

  privacyChanged(hidden: boolean): void { this.privacy = hidden; this.changed(); }

  activity(space: string, from: string, active: boolean): void {
    let people = this.typing.get(space);
    clearTimeout(people?.get(from));
    if (!active) { people?.delete(from); if (!people?.size) this.typing.delete(space); }
    else {
      if (!people) { people = new Map(); this.typing.set(space, people); }
      people.set(from, setTimeout(() => this.activity(space, from, false), TYPING_TTL_MS));
    }
    this.changed();
  }

  clear(): void {
    for (const people of this.typing.values()) for (const timer of people.values()) clearTimeout(timer);
    this.typing.clear(); this.people = {}; this.privacy = null;
  }
}

export function lastSeenLabel(presence: PersonPresence | undefined): string {
  if (!presence) return '';
  if (presence.online) return 'Online';
  if (presence.last_seen_ms === null) return '';
  return 'Visto por último em ' + new Date(presence.last_seen_ms).toLocaleString('pt-BR', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
}
