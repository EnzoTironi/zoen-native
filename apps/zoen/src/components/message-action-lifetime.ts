import type { ClientStore } from '../core/store';
import type { TimelineEntry } from '../core/types';
import type { MessageActionMode } from './use-message-actions';
import { bindMessageDeletion } from './delete-message-action';
import { currentMessageAction, messageActionOwner } from './message-action-current';

type Attempt = Readonly<{
  epoch: number;
  scope: string;
  messageId: string;
  mode?: MessageActionMode;
  emoji?: string;
}>;
type Observation = Readonly<{ activation: number }>;

/** Owns transient interaction attempts, never messages, permissions or receipts. */
export class MessageActionLifetime {
  private latest: { scope: string; entries: readonly TimelineEntry[]; disabled: boolean };
  private focused = false;
  private mounted = { current: false };
  private pending = { current: false };
  private confirmation = { current: null as AbortController | null };
  private activation = { current: 0 };
  private profile: Attempt | null = null;
  private open: Attempt | null = null;
  private observation: Observation = { activation: 0 };
  private listeners = new Set<() => void>();
  private observed: string;
  constructor(
    private store: ClientStore,
    private space: string,
    private scope: string,
    private epoch: number,
    private clearSelection: () => void,
  ) {
    this.observed = scope;
    this.latest = { scope, entries: [], disabled: true };
  }
  getSnapshot = () => this.observation;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  retire = () => {
    ++this.activation.current;
    this.profile = null;
    this.open = null;
    this.confirmation.current?.abort();
    this.observation = { activation: this.activation.current };
    this.clearSelection();
    for (const listener of this.listeners) listener();
  };
  mount() {
    this.mounted.current = true;
  }
  unmount() {
    this.mounted.current = false;
    this.focused = false;
    this.retire();
  }
  focus() {
    this.focused = true;
    this.retire();
  }
  blur() {
    this.focused = false;
    this.retire();
  }
  observe(entries: readonly TimelineEntry[], disabled: boolean) {
    const changed = disabled && !this.latest.disabled;
    this.latest = { scope: this.scope, entries, disabled };
    if (changed) this.retire();
  }
  changed = () => {
    const next = messageActionOwner(this.store, this.store.getSnapshot(), this.space).scope;
    if (next !== this.observed) {
      this.observed = next;
      this.retire();
      return;
    }
    for (const attempt of [this.profile, this.open]) {
      if (!attempt) continue;
      const entry = this.latest.entries.find((value) => value.id === attempt.messageId);
      if (
        !entry ||
        !currentMessageAction(this.store, this.space, this.scope, this.focused, this.latest, entry)
      ) {
        this.retire();
        return;
      }
    }
  };
  current(
    observation: Observation,
    entry: TimelineEntry,
    purpose: 'content' | 'delete' | 'thread' = 'content',
  ) {
    return observation === this.observation && this.mounted.current
      ? currentMessageAction(
          this.store,
          this.space,
          this.scope,
          this.focused,
          this.latest,
          entry,
          purpose,
        )
      : undefined;
  }
  isMounted() {
    return this.mounted.current;
  }
  isPending() {
    return this.pending.current;
  }
  beginPending() {
    this.pending.current = true;
  }
  endPending() {
    this.pending.current = false;
  }
  clearOpen() {
    this.open = null;
  }
  hasProfile() {
    return this.profile !== null;
  }
  startProfile(messageId: string) {
    this.clearOpen();
    const attempt = { epoch: this.activation.current, scope: this.scope, messageId };
    this.profile = attempt;
    return attempt;
  }
  isProfile(attempt: Attempt) {
    return this.profile === attempt && this.activation.current === attempt.epoch;
  }
  finishProfile(attempt: Attempt, navigated: boolean) {
    if (!navigated && this.profile === attempt) this.profile = null;
  }
  opening(messageId: string, mode: MessageActionMode, emoji?: string) {
    return (
      this.open?.messageId === messageId && this.open.mode === mode && this.open.emoji === emoji
    );
  }
  startOpen(messageId: string, mode: MessageActionMode, emoji?: string) {
    const attempt = { epoch: this.activation.current, scope: this.scope, messageId, mode, emoji };
    this.open = attempt;
    return attempt;
  }
  isOpen(attempt: Attempt) {
    return this.open === attempt && this.activation.current === attempt.epoch;
  }
  finishOpen(attempt: Attempt) {
    if (this.open === attempt) this.open = null;
  }
  delete(
    entry: TimelineEntry,
    choose: Parameters<typeof bindMessageDeletion>[3],
    resolve: (entry: TimelineEntry) => TimelineEntry | undefined,
    setBusy: (busy: boolean) => void,
  ) {
    return bindMessageDeletion(this.store, this.space, entry, choose, {
      epoch: this.epoch,
      pending: this.pending,
      mounted: this.mounted,
      activation: this.activation,
      confirmation: this.confirmation,
      resolve,
      setBusy,
    })();
  }
}
