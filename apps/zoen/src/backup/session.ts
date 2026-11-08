import { accountScopeKey, type AccountScope } from '../core/account-scope.ts';
import type { BackupStatusDto } from '../core/generated/BackupStatusDto';
export type BackupTicket = { scope: string; generation: number };
export type BackupState = {
  scope: string;
  phase: 'unavailable' | 'loading' | 'ready' | 'failed';
  data: BackupStatusDto | null;
  key: string;
  saved: boolean;
  password: string;
  repeat: string;
  method: 'recovery' | 'password';
  pending: boolean;
  leaving: boolean;
  failure: string | null;
  status: 'uploaded' | 'enabled' | 'disabled' | 'copied' | null;
};
export interface BackupPort {
  owner(): AccountScope | null;
  status(owner: AccountScope): Promise<BackupStatusDto>;
  generate(owner: AccountScope): Promise<string>;
  enable(owner: AccountScope, key: string): Promise<unknown>;
  password(owner: AccountScope, password: string): Promise<unknown>;
  upload(owner: AccountScope): Promise<unknown>;
  disable(owner: AccountScope): Promise<unknown>;
  copy(key: string): Promise<boolean>;
  schedule(callback: () => void): () => void;
}
export const emptyBackup: BackupState = {
  scope: '',
  phase: 'unavailable',
  data: null,
  key: '',
  saved: false,
  password: '',
  repeat: '',
  method: 'recovery',
  pending: false,
  leaving: false,
  failure: null,
  status: null,
};
/** Ephemeral presentation only; the native gate and completed receipt own backup work. */
export class BackupSession {
  private state: BackupState = emptyBackup;
  private owner: AccountScope | null = null;
  private generation = 0;
  private read = 0;
  private flight: object | null = null;
  private leaving: object | null = null;
  private cancelPoll: (() => void) | null = null;
  private listeners = new Set<() => void>();
  private readonly port: BackupPort;
  constructor(port: BackupPort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (callback: () => void) => {
    this.listeners.add(callback);
    return () => {
      this.listeners.delete(callback);
    };
  };
  private publish(patch: Partial<BackupState>) {
    this.state = { ...this.state, ...patch };
    for (const callback of this.listeners) callback();
  }
  ticket(state = this.state): BackupTicket | null {
    return this.owner && state === this.state
      ? { scope: this.state.scope, generation: this.generation }
      : null;
  }
  current(ticket: BackupTicket | null): ticket is BackupTicket {
    return (
      !!ticket &&
      !!this.owner &&
      ticket.generation === this.generation &&
      ticket.scope === this.state.scope &&
      ticket.scope === accountScopeKey(this.port.owner())
    );
  }
  retire() {
    this.generation++;
    this.read++;
    this.owner = null;
    this.flight = null;
    this.leaving = null;
    this.cancelPoll?.();
    this.cancelPoll = null;
    this.state = { ...emptyBackup };
    for (const callback of this.listeners) callback();
  }
  activate(owner: AccountScope | null) {
    this.retire();
    if (!owner) return;
    this.owner = owner;
    this.publish({ scope: accountScopeKey(owner), phase: 'loading' });
    void this.load(this.ticket());
  }
  async load(ticket: BackupTicket | null) {
    if (!this.current(ticket) || !this.owner || this.state.pending) return;
    const turn = ++this.read,
      owner = this.owner;
    this.cancelPoll?.();
    this.cancelPoll = null;
    this.publish({ phase: 'loading' });
    try {
      const data = await this.port.status(owner);
      if (!this.current(ticket) || turn !== this.read) return;
      this.publish({ data, phase: 'ready' });
      if (data.in_progress)
        this.cancelPoll = this.port.schedule(() => {
          this.cancelPoll = null;
          if (this.current(ticket)) void this.load(ticket);
        });
    } catch {
      if (this.current(ticket) && turn === this.read) this.publish({ phase: 'failed' });
    }
  }
  change(
    ticket: BackupTicket | null,
    patch: Partial<Pick<BackupState, 'password' | 'repeat' | 'saved' | 'method'>>,
  ) {
    if (!this.current(ticket) || this.state.pending || this.state.data?.in_progress) return;
    if (patch.method && this.state.key) return;
    this.publish({ ...patch, failure: null, status: null });
  }
  private async perform<T>(
    ticket: BackupTicket | null,
    action: (owner: AccountScope) => Promise<T>,
    applied: (value: T) => Partial<BackupState>,
    refresh = true,
  ) {
    if (
      !this.current(ticket) ||
      !this.owner ||
      this.flight ||
      this.state.pending ||
      this.state.data?.in_progress ||
      (refresh && this.state.phase !== 'ready')
    )
      return;
    const attempt = {},
      owner = this.owner;
    this.flight = attempt;
    if (refresh) {
      this.read++;
      this.cancelPoll?.();
      this.cancelPoll = null;
    }
    this.publish({ pending: true, failure: null, status: null });
    try {
      const value = await action(owner);
      if (this.current(ticket) && this.flight === attempt) this.publish(applied(value));
    } catch {
      if (this.current(ticket) && this.flight === attempt) this.publish({ failure: 'operation' });
    } finally {
      if (this.flight === attempt) {
        this.flight = null;
        if (this.current(ticket)) {
          this.publish({ pending: false });
          if (refresh) void this.load(ticket);
        }
      }
    }
  }
  generate(ticket: BackupTicket | null) {
    if (this.state.key || this.state.data?.enabled || this.state.method !== 'recovery') return;
    return this.perform(
      ticket,
      (owner) => this.port.generate(owner),
      (key) => ({ key, saved: false }),
    );
  }
  enable(ticket: BackupTicket | null) {
    const { key, saved } = this.state;
    if (!key || !saved || this.state.method !== 'recovery') return;
    return this.perform(
      ticket,
      (owner) => this.port.enable(owner, key),
      () => ({ key: '', saved: false, status: 'enabled' }),
    );
  }
  password(ticket: BackupTicket | null) {
    const { password, repeat } = this.state;
    if (
      this.state.data?.enabled ||
      this.state.method !== 'password' ||
      password.length < 12 ||
      password !== repeat
    )
      return;
    return this.perform(
      ticket,
      (owner) => this.port.password(owner, password),
      () => ({ password: '', repeat: '', status: 'enabled' }),
    );
  }
  upload(ticket: BackupTicket | null) {
    if (!this.state.data?.enabled) return;
    return this.perform(
      ticket,
      (owner) => this.port.upload(owner),
      () => ({ status: 'uploaded' }),
    );
  }
  disable(ticket: BackupTicket | null, confirm: () => Promise<boolean>) {
    if (!this.state.data?.enabled) return;
    return this.perform(
      ticket,
      async (owner) => {
        if (!(await confirm()) || !this.current(ticket)) return false;
        await this.port.disable(owner);
        return true;
      },
      (accepted) => (accepted ? { status: 'disabled' } : {}),
    );
  }
  copy(ticket: BackupTicket | null) {
    const key = this.state.key;
    if (!key) return;
    return this.perform(
      ticket,
      async () => {
        if (!(await this.port.copy(key))) throw Error('copy refused');
      },
      () => ({ status: 'copied' }),
      false,
    );
  }
  async leave(ticket: BackupTicket | null, confirm: () => Promise<boolean>, leave: () => void) {
    if (!this.current(ticket) || this.state.pending || this.state.data?.in_progress || this.leaving)
      return;
    const attempt = {};
    this.leaving = attempt;
    try {
      if (!(await confirm()) || !this.current(ticket) || this.leaving !== attempt) return;
      this.publish({ key: '', saved: false, password: '', repeat: '', leaving: true });
      leave();
    } finally {
      if (this.leaving === attempt) this.leaving = null;
    }
  }
}
