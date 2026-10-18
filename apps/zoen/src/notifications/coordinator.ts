import { t } from '../i18n/index.ts';
export type PushPermission = 'unknown' | 'undetermined' | 'denied' | 'allowed';
export type PushConfiguration = { environment: 'sandbox' | 'production'; topic: string };
export type PushRegistrationState =
  | 'unsupported'
  | 'unregistered'
  | 'registering'
  | 'acknowledged'
  | 'waiting'
  | 'refused';
export type PushState = {
  permission: PushPermission;
  registration: PushRegistrationState;
  busy: boolean;
  failure: string | null;
  /** The legacy register_push acknowledgement does not attest to provider availability. */
  deliveryVerified: false;
};
export interface PushPorts {
  permission(request: boolean): Promise<Exclude<PushPermission, 'unknown'>>;
  token(): Promise<string>;
  register(
    configuration: PushConfiguration,
    token: string,
  ): Promise<{ kind: 'acknowledged' } | { kind: 'refused' }>;
  current(): boolean;
  schedule?(callback: () => void, milliseconds: number): () => void;
}

export function apnsToken(value: unknown): string | null {
  return typeof value === 'string' &&
    value.length >= 2 &&
    value.length <= 512 &&
    value.length % 2 === 0 &&
    /^[a-f0-9]+$/i.test(value)
    ? value.toLowerCase()
    : null;
}

export function pushConfiguration(value: unknown): PushConfiguration | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const input = value as Record<string, unknown>;
  if (
    !['sandbox', 'production'].includes(String(input.environment)) ||
    typeof input.topic !== 'string' ||
    input.topic.length > 255 ||
    !/^[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+$/.test(input.topic)
  )
    return null;
  return { environment: input.environment as PushConfiguration['environment'], topic: input.topic };
}

/** Owns only transient registration work. Durable queue/ack state belongs to Rust. */
export class PushCoordinator {
  private state: PushState;
  private listeners = new Set<() => void>();
  private epoch = 0;
  private permissionRevision = 0;
  private disposed = false;
  private desired: { token: string; revision: number } | null = null;
  private acknowledged: string | undefined;
  private revision = 0;
  private pump: Promise<void> | null = null;
  private cancelRetry: (() => void) | null = null;
  private retry = 0;

  private readonly ports: PushPorts;
  private readonly configuration: PushConfiguration | null;
  constructor(ports: PushPorts, configuration: PushConfiguration | null) {
    this.ports = ports;
    this.configuration = configuration;
    this.state = {
      permission: 'unknown',
      registration: configuration ? 'unregistered' : 'unsupported',
      busy: false,
      failure: null,
      deliveryVerified: false,
    };
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private active(stamp = this.epoch) {
    return !this.disposed && stamp === this.epoch && this.ports.current();
  }
  private publish(patch: Partial<PushState>) {
    if (!this.active()) return;
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

  async foreground(requestPermission = false) {
    if (!this.active()) return;
    const revision = ++this.permissionRevision;
    const stamp = this.epoch;
    this.publish({ busy: true, failure: null });
    try {
      const permission = await this.ports.permission(requestPermission);
      if (!this.active(stamp) || revision !== this.permissionRevision) return;
      this.publish({ permission });
      if (!this.configuration) return;
      if (permission === 'denied') {
        await this.change('');
        return;
      }
      if (permission !== 'allowed') return;
      const token = await this.ports.token();
      if (!this.active(stamp) || revision !== this.permissionRevision) return;
      const canonical = apnsToken(token);
      if (!canonical) throw new Error('invalid token');
      await this.change(canonical);
    } catch {
      if (this.active(stamp) && revision === this.permissionRevision)
        this.publish({
          failure: t('notifications.notificationsCouldNotBeSetUpNowTry'),
        });
    } finally {
      if (this.active(stamp) && revision === this.permissionRevision) this.publish({ busy: false });
    }
  }

  async receivedToken(value: unknown) {
    if (!this.active() || this.state.permission !== 'allowed' || !this.configuration) return;
    const token = apnsToken(value);
    if (!token) {
      this.publish({ failure: t('notifications.theDeviceDidNotReturnAValidNotification') });
      return;
    }
    await this.change(token);
  }

  private async change(token: string) {
    if (!this.active() || !this.configuration || (this.acknowledged === token && !this.desired))
      return;
    if (this.desired?.token !== token) this.desired = { token, revision: ++this.revision };
    this.cancelRetry?.();
    this.cancelRetry = null;
    this.retry = 0;
    await this.flush();
  }

  private flush(): Promise<void> {
    if (this.pump) return this.pump;
    if (!this.active() || !this.configuration || !this.desired) return Promise.resolve();
    const stamp = this.epoch;
    const configuration = this.configuration;
    const operation = (async () => {
      while (this.active(stamp) && this.desired) {
        const desired = this.desired;
        this.publish({ registration: 'registering', failure: null });
        try {
          const result = await this.ports.register(configuration, desired.token);
          if (!this.active(stamp)) return;
          if (desired.revision !== this.desired?.revision) continue;
          if (result.kind === 'refused') {
            this.desired = null;
            this.publish({
              registration: 'refused',
              failure: t('notifications.theServerRefusedNotificationRegistrationForThisDevice'),
            });
            return;
          }
          this.acknowledged = desired.token;
          this.desired = null;
          this.retry = 0;
          this.publish({
            registration: desired.token ? 'acknowledged' : 'unregistered',
            failure: null,
          });
        } catch {
          if (!this.active(stamp)) return;
          if (desired.revision !== this.desired?.revision) continue;
          // A failed transport does not prove a refusal, a durable queue, or delivery.
          this.publish({
            registration: 'waiting',
            failure: t('notifications.registrationHasNotBeenConfirmedByTheServer'),
          });
          this.scheduleRetry(stamp);
          return;
        }
      }
    })();
    this.pump = operation.finally(() => {
      this.pump = null;
      if (stamp !== this.epoch && this.active() && this.desired) void this.flush();
    });
    return this.pump;
  }

  private scheduleRetry(stamp: number) {
    if (this.retry >= 5) return;
    const milliseconds = 10_000 * 2 ** this.retry++;
    const schedule =
      this.ports.schedule ??
      ((callback, delay) => {
        const timer = setTimeout(callback, delay);
        return () => clearTimeout(timer);
      });
    this.cancelRetry = schedule(() => {
      this.cancelRetry = null;
      if (this.active(stamp)) void this.flush();
    }, milliseconds);
  }

  /** Disposal invalidates callbacks; it does not unregister a device merely because the app closed. */
  activate() {
    this.disposed = false;
  }

  dispose() {
    this.disposed = true;
    this.epoch += 1;
    this.permissionRevision += 1;
    this.cancelRetry?.();
    this.cancelRetry = null;
    this.desired = null;
    this.listeners.clear();
  }
}
