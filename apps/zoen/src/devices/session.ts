import type { ClientStore } from '../core/store';
import type { DeviceDto } from '../core/generated/DeviceDto';
import { accountScope, accountScopeKey, type AccountScope } from '../core/account-scope.ts';

export type DeviceTicket = Readonly<{ scope: string; generation: number }>;
export type DeviceState = Readonly<{
  scope: string;
  phase: 'inactive' | 'loading' | 'ready' | 'failed';
  devices: readonly DeviceDto[];
  selected: string | null;
  pending: string | null;
  readFailed: boolean;
  revokeFailed: boolean;
}>;
export interface DevicePort {
  scope(): AccountScope | null;
  read(): Promise<DeviceDto[]>;
  revoke(device: string): Promise<unknown>;
}
export const emptyDevices: DeviceState = {
  scope: '',
  phase: 'inactive',
  devices: [],
  selected: null,
  pending: null,
  readFailed: false,
  revokeFailed: false,
};
export function createDevicePort(store: ClientStore): DevicePort {
  return {
    scope: () => accountScope(store),
    read: () => store.driver.request({ op: 'devices' }),
    revoke: (device_id) => store.request({ op: 'unlink_device', device_id }),
  };
}

function validDirectory(devices: DeviceDto[], current: string | undefined): boolean {
  if (!Array.isArray(devices)) return false;
  const ids = new Set<string>();
  for (const device of devices) {
    if (
      !device ||
      typeof device.device_id !== 'string' ||
      !/^[a-f0-9]{64}$/.test(device.device_id) ||
      ids.has(device.device_id) ||
      typeof device.revoked !== 'boolean' ||
      typeof device.this_device !== 'boolean' ||
      device.this_device !== (device.device_id === current)
    )
      return false;
    ids.add(device.device_id);
  }
  return devices.some((device) => device.this_device);
}

/** A disposable projection. The authenticated Core directory remains authoritative. */
export class DeviceSession {
  private state = emptyDevices;
  private owner: AccountScope | null = null;
  private generation = 0;
  private operation = 0;
  private listeners = new Set<() => void>();
  private readonly port: DevicePort;
  constructor(port: DevicePort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<DeviceState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  activate(owner: AccountScope | null) {
    this.owner = owner ? Object.freeze({ ...owner }) : null;
    ++this.generation;
    ++this.operation;
    this.state = { ...emptyDevices, scope: accountScopeKey(owner) };
    this.publish({});
  }
  retire() {
    this.activate(null);
  }
  private current(generation = this.generation) {
    return Boolean(
      this.owner &&
      generation === this.generation &&
      accountScopeKey(this.owner) === accountScopeKey(this.port.scope()),
    );
  }
  ticket(): DeviceTicket | null {
    return this.current()
      ? Object.freeze({ scope: this.state.scope, generation: this.generation })
      : null;
  }
  private accepts(ticket: DeviceTicket | null) {
    return Boolean(ticket && ticket.scope === this.state.scope && this.current(ticket.generation));
  }
  async load(ticket: DeviceTicket | null, reconcile = false) {
    if (!this.accepts(ticket) || (this.state.pending && !reconcile)) return;
    const generation = ++this.generation;
    this.publish({ phase: 'loading', readFailed: false });
    try {
      const devices = await this.port.read();
      if (!this.current(generation)) return;
      if (!validDirectory(devices, this.owner?.device)) throw new Error('Invalid device directory');
      this.publish({
        phase: 'ready',
        devices: devices.map((device) => ({ ...device })),
        revokeFailed:
          reconcile &&
          this.state.revokeFailed &&
          devices.some((device) => device.device_id === this.state.pending && !device.revoked),
        selected: devices.some((device) => device.device_id === this.state.selected)
          ? this.state.selected
          : null,
      });
    } catch {
      if (this.current(generation))
        this.publish({ phase: 'failed', readFailed: true, devices: [], selected: null });
    }
  }
  select(device: string | null, ticket: DeviceTicket | null) {
    if (!this.accepts(ticket) || (device !== null && this.state.pending)) return;
    if (
      device &&
      (this.state.phase !== 'ready' || !this.state.devices.some((row) => row.device_id === device))
    )
      return;
    this.publish({ selected: device });
  }
  async revoke(device: string, confirm: () => Promise<boolean>, ticket: DeviceTicket | null) {
    const row = this.state.devices.find((value) => value.device_id === device);
    if (
      !this.accepts(ticket) ||
      this.state.phase !== 'ready' ||
      this.state.pending ||
      !row ||
      row.this_device ||
      row.revoked ||
      this.state.selected !== device
    )
      return;
    const generation = this.generation;
    const operation = ++this.operation;
    this.publish({ pending: device, revokeFailed: false });
    try {
      const accepted = await confirm();
      if (!accepted || !this.current(generation) || this.state.selected !== device) return;
      await this.port.revoke(device);
      if (this.current(generation)) await this.load(this.ticket(), true);
    } catch {
      if (this.current(generation)) {
        this.publish({ revokeFailed: true });
        // A lost response may follow acceptance; reconcile without another revoke.
        await this.load(this.ticket(), true);
      }
    } finally {
      if (operation === this.operation && this.current()) this.publish({ pending: null });
    }
  }
}
