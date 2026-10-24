import { nativeLocationPoint } from './native-location.ts';
import type { MediaDriver, StaticLocation } from './types.ts';

type State = Readonly<{
  binding: string | null;
  point: StaticLocation | null;
  results: readonly StaticLocation[];
  busy: boolean;
  error: boolean;
}>;
/** Transient map/search selection. Only the parent's explicit Add writes its existing journal. */
export class LocationSelection {
  private state: State = { binding: null, point: null, results: [], busy: false, error: false };
  private generation = 0;
  private readonly listeners = new Set<() => void>();
  private readonly ports: {
    media: MediaDriver;
    current(): boolean;
    gps(current: () => boolean): Promise<StaticLocation | null>;
  };
  constructor(ports: LocationSelection['ports']) {
    this.ports = ports;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(next: State) {
    this.state = next;
    for (const listener of this.listeners) listener();
  }
  current(binding: string | null): boolean {
    return Boolean(binding && binding === this.state.binding && this.ports.current());
  }
  activate(binding: string) {
    this.retire();
    if (this.ports.current()) this.publish({ ...this.state, binding });
  }
  retire() {
    ++this.generation;
    const old = this.state.binding;
    this.publish({ binding: null, point: null, results: [], busy: false, error: false });
    if (old) this.ports.media.cancelLocationSearch?.(old);
  }
  select(binding: string, point: StaticLocation) {
    if (!this.current(binding)) return;
    const value = nativeLocationPoint(point);
    ++this.generation;
    this.ports.media.cancelLocationSearch?.(binding);
    this.publish({ ...this.state, point: value, results: [], busy: false, error: false });
  }
  private async request(
    binding: string,
    read: () => Promise<readonly StaticLocation[] | null>,
    search: boolean,
  ) {
    if (!this.current(binding) || this.state.busy) return;
    const generation = ++this.generation;
    const valid = () => generation === this.generation && this.current(binding);
    this.publish({ ...this.state, busy: true, error: false });
    try {
      const result = await read();
      if (!valid()) return;
      const points = result?.map(nativeLocationPoint) ?? [];
      if (points.length > 20) throw Error('location-invalid');
      this.publish({
        ...this.state,
        ...(search ? { results: points } : { point: points[0] ?? null, results: [] }),
        busy: false,
      });
    } catch {
      if (valid()) this.publish({ ...this.state, busy: false, error: true });
    }
  }
  search(binding: string, query: string) {
    return this.request(
      binding,
      async () => {
        if (!this.ports.media.searchLocations) throw Error('location-unavailable');
        return this.ports.media.searchLocations(binding, query);
      },
      true,
    );
  }
  gps(binding: string) {
    const generation = this.generation + 1;
    return this.request(
      binding,
      async () => {
        const point = await this.ports.gps(
          () => generation === this.generation && this.current(binding),
        );
        return point ? [point] : null;
      },
      false,
    );
  }
}
