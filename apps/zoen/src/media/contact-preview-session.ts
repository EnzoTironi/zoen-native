import { materializeObservedFile } from './observed-file.ts';
import type { ContactPreview, LocalMediaFile, MediaDriver } from './types.ts';

type Copy = {
  file: LocalMediaFile;
  users: number;
  retired: boolean;
  release: Promise<void> | null;
  done: Promise<void>;
  finish(): void;
};
export type ContactPreviewState = Readonly<{
  visible: boolean;
  ticket: number;
  phase: 'idle' | 'loading' | 'ready' | 'error';
  contact: ContactPreview | null;
  busy: boolean;
}>;

/** An ephemeral viewer of one verified file, never a contact/message/draft store. */
export class ContactPreviewSession {
  private state: ContactPreviewState = {
    visible: false,
    ticket: 0,
    phase: 'idle',
    contact: null,
    busy: false,
  };
  private readonly listeners = new Set<() => void>();
  private generation = 0;
  private owned: Copy | null = null;
  private working = false;
  private pending: Promise<void> | null = null;
  private readonly copies = new Set<Copy>();
  private readonly ports: {
    media: MediaDriver;
    source: { itemId: string; version?: number; expectedHash: string };
    current(): boolean;
  };
  constructor(ports: ContactPreviewSession['ports']) {
    this.ports = ports;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(next: ContactPreviewState) {
    this.state = next;
    for (const listener of this.listeners) listener();
  }
  private async release(copy: Copy) {
    if (!copy.retired) return;
    if (copy.users === 0) {
      await (copy.release ??= this.ports.media.releaseLocalFile(copy.file.uri).finally(() => {
        this.copies.delete(copy);
        copy.finish();
      }));
    } else await copy.done;
  }
  open(): Promise<void> {
    if (this.pending || this.working || this.state.visible || !this.ports.current())
      return Promise.resolve();
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
    this.publish({
      visible: true,
      ticket: generation,
      phase: 'loading',
      contact: null,
      busy: false,
    });
    let copy: Copy | null = null;
    try {
      const read = this.ports.media.readContact;
      if (!read) throw Error('contact-preview-unavailable');
      const file = await materializeObservedFile(this.ports.media, this.ports.source, current);
      let finish!: () => void;
      const done = new Promise<void>((resolve) => {
        finish = resolve;
      });
      copy = { file, users: 0, retired: true, release: null, done, finish };
      this.copies.add(copy);
      if (!current()) return;
      const parsed = await read(file);
      if (!current()) return;
      const contact = Object.freeze({
        name: parsed.name,
        phones: Object.freeze(parsed.phones.map((phone) => Object.freeze({ ...phone }))),
      });
      copy.retired = false;
      this.owned = copy;
      copy = null;
      this.publish({ visible: true, ticket: generation, phase: 'ready', contact, busy: false });
    } catch {
      if (current())
        this.publish({
          visible: true,
          ticket: generation,
          phase: 'error',
          contact: null,
          busy: false,
        });
    } finally {
      if (copy) await this.release(copy);
    }
  }
  /** The callback may present an OS action only after its own awaited readiness recheck. */
  async perform(
    expected: ContactPreview,
    work: (file: LocalMediaFile, current: () => boolean) => Promise<void>,
  ): Promise<void> {
    const copy = this.owned;
    if (
      this.working ||
      !copy ||
      this.state.contact !== expected ||
      this.state.phase !== 'ready' ||
      !this.state.visible ||
      !this.ports.current()
    )
      return;
    const generation = this.generation;
    const current = () => generation === this.generation && !copy.retired && this.ports.current();
    this.working = true;
    copy.users++;
    this.publish({ ...this.state, busy: true });
    try {
      if (current()) await work(copy.file, current);
    } catch {
      if (current()) this.publish({ ...this.state, phase: 'error' });
    } finally {
      copy.users--;
      await this.release(copy);
      this.working = false;
      if (current()) this.publish({ ...this.state, busy: false });
    }
  }
  dismiss(ticket: number): Promise<void> {
    return ticket === this.state.ticket ? this.close(true) : Promise.resolve();
  }
  async close(retainSummary = false): Promise<void> {
    ++this.generation;
    this.owned = null;
    const contact = retainSummary && this.ports.current() ? this.state.contact : null;
    this.publish({
      visible: false,
      ticket: this.generation,
      phase: 'idle',
      contact,
      busy: this.working || this.pending !== null,
    });
    for (const copy of this.copies) copy.retired = true;
    await this.pending;
    await Promise.all([...this.copies].map((copy) => this.release(copy)));
    if (!this.state.visible && this.ports.current()) this.publish({ ...this.state, busy: false });
  }
}
