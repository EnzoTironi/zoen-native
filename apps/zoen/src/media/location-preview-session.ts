import { materializeObservedFile } from './observed-file.ts';
import { nativeLocationPoint } from './native-location.ts';
import type { LocalMediaFile, MediaDriver, StaticLocation } from './types.ts';

type State = Readonly<{
  visible: boolean;
  ticket: number;
  phase: 'idle' | 'loading' | 'ready' | 'error';
  point: StaticLocation | null;
}>;
/** Holds only the existing authorized preview copy while its read-only map is visible. */
export class LocationPreviewSession {
  private state: State = { visible: false, ticket: 0, phase: 'idle', point: null };
  private readonly listeners = new Set<() => void>();
  private generation = 0;
  private owned: LocalMediaFile | null = null;
  private pending: Promise<void> | null = null;
  private readonly ports: {
    media: MediaDriver;
    source: { itemId: string; version?: number; expectedHash: string };
    current(): boolean;
  };
  constructor(ports: LocationPreviewSession['ports']) {
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
  open(): Promise<void> {
    if (this.pending || this.state.visible || !this.ports.current()) return Promise.resolve();
    const generation = ++this.generation;
    const run = Promise.resolve().then(() => this.read(generation));
    this.pending = run;
    return run.finally(() => {
      if (this.pending === run) this.pending = null;
    });
  }
  private async read(generation: number) {
    const current = () => generation === this.generation && this.ports.current();
    if (!current()) return;
    this.publish({ visible: true, ticket: generation, phase: 'loading', point: null });
    let file: LocalMediaFile | null = null;
    try {
      if (!this.ports.media.readLocation) throw Error('location-unavailable');
      file = await materializeObservedFile(this.ports.media, this.ports.source, current);
      if (!current()) return;
      const point = nativeLocationPoint(await this.ports.media.readLocation(file));
      if (!current()) return;
      this.owned = file;
      file = null;
      this.publish({ visible: true, ticket: generation, phase: 'ready', point });
    } catch {
      if (current())
        this.publish({ visible: true, ticket: generation, phase: 'error', point: null });
    } finally {
      if (file) await this.ports.media.releaseLocalFile(file.uri);
    }
  }
  dismiss(ticket: number): Promise<void> {
    return ticket === this.state.ticket ? this.close() : Promise.resolve();
  }
  async close(): Promise<void> {
    ++this.generation;
    const file = this.owned;
    this.owned = null;
    this.publish({ visible: false, ticket: this.generation, phase: 'idle', point: null });
    await this.pending;
    if (file) await this.ports.media.releaseLocalFile(file.uri);
  }
}
