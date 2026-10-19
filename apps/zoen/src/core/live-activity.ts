import { t } from '../i18n/index.ts';
import type { CoreEvent, Snapshot } from './types.ts';

export const TYPING_TTL_MS = 6000;
export const TYPING_REFRESH_MS = 2000;
export const TYPING_IDLE_MS = 4000;
export interface ActivityClock {
  now(): number;
  setTimeout(callback: () => void, delay: number): ReturnType<typeof setTimeout>;
  clearTimeout(timer: ReturnType<typeof setTimeout>): void;
}
export const activityClock: ActivityClock = {
  now: () => Date.now(),
  setTimeout: (callback, delay) => setTimeout(callback, delay),
  clearTimeout: (timer) => clearTimeout(timer),
};
export type ActivityStatus = { kind: 'processing' | 'building' | 'call'; expiresAt: number };
const statuses: Record<string, { kind: ActivityStatus['kind']; ttl: number }> = {
  processing: { kind: 'processing', ttl: 60_000 },
  building: { kind: 'building', ttl: 120_000 },
  in_call: { kind: 'call', ttl: 3_600_000 },
  call: { kind: 'call', ttl: 3_600_000 },
};
export type LiveActivityState = {
  online: Readonly<Record<string, true>>;
  typing: Readonly<Record<string, Readonly<Record<string, number>>>>;
  statuses: Readonly<Record<string, Readonly<Record<string, ActivityStatus>>>>;
};
export const emptyActivity = (): LiveActivityState => ({ online: {}, typing: {}, statuses: {} });
const connected = (snapshot: Snapshot | null) =>
  snapshot?.connection.state === 'online' && snapshot.connection.synced === true;
function ownId(snapshot: Snapshot | null) {
  return snapshot?.me?.id ?? snapshot?.account?.identity_id;
}
function member(snapshot: Snapshot | null, identity: string, space?: string) {
  if (!identity || identity === ownId(snapshot)) return false;
  return (
    snapshot?.conversations.some(
      (chat) =>
        (!space || chat.id === space) && chat.members.some((person) => person.id === identity),
    ) ?? false
  );
}

/** Relay session hints only: no timestamps become last-seen or durable profile state. */
export class LiveActivity {
  private online = new Set<string>();
  private typing = new Map<string, Map<string, number>>();
  private statuses = new Map<string, Map<string, ActivityStatus>>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private enabled = false;
  private waiting = false;
  private pending: { event: Extract<CoreEvent, { kind: 'presence' | 'ephemeral' }>; at: number }[] =
    [];
  private readonly scope: () => Snapshot | null;
  private readonly changed: (value: LiveActivityState) => void;
  private readonly clock: ActivityClock;
  constructor(
    scope: () => Snapshot | null,
    changed: (value: LiveActivityState) => void,
    clock: ActivityClock = activityClock,
  ) {
    this.scope = scope;
    this.changed = changed;
    this.clock = clock;
  }
  value(): LiveActivityState {
    return {
      online: Object.fromEntries([...this.online].map((id) => [id, true as const])),
      typing: Object.fromEntries(
        [...this.typing].map(([space, people]) => [space, Object.fromEntries(people)]),
      ),
      statuses: Object.fromEntries(
        [...this.statuses].map(([space, people]) => [space, Object.fromEntries(people)]),
      ),
    };
  }
  isConnected() {
    return this.enabled;
  }
  beginConnection() {
    this.reset();
    this.waiting = true;
  }
  synchronize(snapshot: Snapshot) {
    this.enabled = connected(snapshot);
    if (!this.enabled) {
      this.reset();
      return;
    }
    for (const id of this.online) if (!member(snapshot, id)) this.online.delete(id);
    for (const [space, people] of this.typing) {
      for (const id of people.keys()) if (!member(snapshot, id, space)) people.delete(id);
      if (!people.size) this.typing.delete(space);
    }
    for (const [space, people] of this.statuses) {
      for (const id of people.keys()) if (!member(snapshot, id, space)) people.delete(id);
      if (!people.size) this.statuses.delete(space);
    }
    this.waiting = false;
    for (const hint of this.pending) this.apply(hint.event, snapshot, hint.at);
    this.pending = [];
    this.schedule();
  }
  receive(event: Extract<CoreEvent, { kind: 'presence' | 'ephemeral' }>) {
    if (!this.enabled) {
      if (this.waiting) {
        if (this.pending.length >= 256) this.pending.shift();
        this.pending.push({ event, at: this.clock.now() });
      }
      return;
    }
    const snapshot = this.scope();
    if (!connected(snapshot)) return;
    this.apply(event, snapshot, this.clock.now());
    this.schedule();
    this.changed(this.value());
  }
  private apply(
    event: Extract<CoreEvent, { kind: 'presence' | 'ephemeral' }>,
    snapshot: Snapshot | null,
    at: number,
  ) {
    if (event.kind === 'presence') {
      if (!member(snapshot, event.identityId)) return;
      if (event.online) this.online.add(event.identityId);
      else {
        this.online.delete(event.identityId);
        for (const [space, people] of this.typing) {
          people.delete(event.identityId);
          if (!people.size) this.typing.delete(space);
        }
        for (const [space, people] of this.statuses) {
          people.delete(event.identityId);
          if (!people.size) this.statuses.delete(space);
        }
      }
    } else {
      if (
        !['typing', 'stopped', 'status'].includes(event.type) ||
        !member(snapshot, event.fromId, event.spaceId)
      )
        return;
      if (event.type === 'status') {
        const status = Object.hasOwn(statuses, event.detail) ? statuses[event.detail] : undefined;
        let people = this.statuses.get(event.spaceId);
        if (status && at + status.ttl > this.clock.now()) {
          if (!people) {
            people = new Map();
            this.statuses.set(event.spaceId, people);
          }
          people.set(event.fromId, { kind: status.kind, expiresAt: at + status.ttl });
          this.typing.get(event.spaceId)?.delete(event.fromId);
          if (!this.typing.get(event.spaceId)?.size) this.typing.delete(event.spaceId);
        } else people?.delete(event.fromId);
        if (people && !people.size) this.statuses.delete(event.spaceId);
        return;
      }
      let people = this.typing.get(event.spaceId);
      if (event.type === 'typing') {
        if (at + TYPING_TTL_MS <= this.clock.now()) return;
        if (!people) {
          people = new Map();
          this.typing.set(event.spaceId, people);
        }
        people.set(event.fromId, at + TYPING_TTL_MS);
        this.statuses.get(event.spaceId)?.delete(event.fromId);
        if (!this.statuses.get(event.spaceId)?.size) this.statuses.delete(event.spaceId);
      } else people?.delete(event.fromId);
      if (people && !people.size) this.typing.delete(event.spaceId);
    }
  }
  reset() {
    this.enabled = false;
    this.waiting = false;
    this.pending = [];
    if (this.timer !== undefined) this.clock.clearTimeout(this.timer);
    this.timer = undefined;
    this.online.clear();
    this.typing.clear();
    this.statuses.clear();
  }
  private schedule() {
    if (this.timer !== undefined) this.clock.clearTimeout(this.timer);
    this.timer = undefined;
    const deadlines = [...this.typing.values()].flatMap((people) => [...people.values()]);
    deadlines.push(
      ...[...this.statuses.values()].flatMap((people) =>
        [...people.values()].map((status) => status.expiresAt),
      ),
    );
    if (!deadlines.length) return;
    this.timer = this.clock.setTimeout(
      () => {
        this.timer = undefined;
        const now = this.clock.now();
        for (const [space, people] of this.typing) {
          for (const [id, deadline] of people) if (deadline <= now) people.delete(id);
          if (!people.size) this.typing.delete(space);
        }
        for (const [space, people] of this.statuses) {
          for (const [id, status] of people) if (status.expiresAt <= now) people.delete(id);
          if (!people.size) this.statuses.delete(space);
        }
        this.changed(this.value());
        this.schedule();
      },
      Math.max(0, Math.min(...deadlines) - this.clock.now()),
    );
  }
}

/** Throttles best-effort hints without going through ClientStore.request/refresh. */
export class TypingSender {
  private spaces = new Map<string, { last: number; timer: ReturnType<typeof setTimeout> }>();
  private serial = Promise.resolve();
  private generation = 0;
  private readonly allowed: (space: string) => boolean;
  private readonly send: (space: string, typing: boolean) => Promise<unknown>;
  private readonly clock: ActivityClock;
  constructor(
    allowed: (space: string) => boolean,
    send: (space: string, typing: boolean) => Promise<unknown>,
    clock: ActivityClock = activityClock,
  ) {
    this.allowed = allowed;
    this.send = send;
    this.clock = clock;
  }
  update(space: string, typing: boolean) {
    const current = this.spaces.get(space);
    if (!typing || !this.allowed(space)) {
      if (!current) return;
      this.clock.clearTimeout(current.timer);
      this.spaces.delete(space);
      if (this.allowed(space)) this.enqueue(space, false);
      return;
    }
    if (current) this.clock.clearTimeout(current.timer);
    const now = this.clock.now();
    const next = {
      last: current?.last ?? -Infinity,
      timer: this.clock.setTimeout(() => this.update(space, false), TYPING_IDLE_MS),
    };
    if (now - next.last >= TYPING_REFRESH_MS) {
      next.last = now;
      this.enqueue(space, true);
    }
    this.spaces.set(space, next);
  }
  reset() {
    ++this.generation;
    for (const value of this.spaces.values()) this.clock.clearTimeout(value.timer);
    this.spaces.clear();
    this.serial = Promise.resolve();
  }
  private enqueue(space: string, typing: boolean) {
    const generation = this.generation;
    this.serial = this.serial
      .then(async () => {
        if (generation === this.generation && this.allowed(space)) await this.send(space, typing);
      })
      .catch(() => {
        /* Transient hints never replace the user's text with an error. */
      });
  }
}

export function chatActivityLabel(
  snapshot: Snapshot | null,
  activity: LiveActivityState,
  space: string,
): string | null {
  const chat = snapshot?.conversations.find((chat) => chat.id === space);
  if (!chat) return null;
  const people = chat.members.filter(
    (person) => person.id !== ownId(snapshot) && activity.typing[space]?.[person.id] !== undefined,
  );
  if (people.length === 1)
    return chat.kind === 'Direct'
      ? t('presentation.activityTyping')
      : t('presentation.activityPersonTyping', { name: people[0]!.name });
  if (people.length === 2)
    return t('presentation.activityPairTyping', {
      first: people[0]!.name,
      second: people[1]!.name,
    });
  if (people.length > 2) return t('presentation.activityPeopleTyping', { count: people.length });
  const active = chat.members.find(
    (person) => person.id !== ownId(snapshot) && activity.statuses[space]?.[person.id],
  );
  if (active) {
    const status = activity.statuses[space]![active.id]!;
    if (chat.kind === 'Direct')
      return {
        processing: t('presentation.activityProcessing'),
        building: t('presentation.activityBuilding'),
        call: t('presentation.activityCall'),
      }[status.kind];
    return {
      processing: t('presentation.activityPersonProcessing', { name: active.name }),
      building: t('presentation.activityPersonBuilding', { name: active.name }),
      call: t('presentation.activityPersonCall', { name: active.name }),
    }[status.kind];
  }
  return chat.kind === 'Direct' && chat.counterpart && activity.online[chat.counterpart.id]
    ? t('presentation.activityOnline')
    : null;
}
