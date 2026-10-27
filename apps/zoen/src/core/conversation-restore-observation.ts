import type { ConversationNativeMetrics } from './conversation-reveal';

export type RestoreObservationContext = Readonly<{
  searchActive: boolean;
  reply: boolean;
  editing: boolean;
  keyboardHeight: number;
  keyboardProgress: number;
  list: {
    firstItemOffset: number;
    childHeight: number;
    windowHeight: number;
    lastRowY: number | null;
    lastRowHeight: number | null;
  } | null;
}>;

/** Opt-in QA observation only: no message/query/owner IDs, scroll command or
 * retained content. A retired activation can never publish a late sample. */
export class ConversationRestoreObservation {
  private live = true;
  private sequence = -1n;
  private searched = false;
  private last = '';
  private composerHeight: number | null = null;
  private requestedInset: { top: number; bottom: number } | null = null;
  private revision = 0;
  private counts = { 'before-search': 0, search: 0, restored: 0 };
  private readonly binding: string;
  private readonly activation: number;
  private port: {
    current(): boolean;
    context(): RestoreObservationContext;
    emit(record: Readonly<Record<string, unknown>>): void;
  } | null = null;
  constructor(binding: string, activation: number) {
    this.binding = binding;
    this.activation = activation;
  }
  connect(port: NonNullable<ConversationRestoreObservation['port']>) {
    if (this.live) this.port = port;
  }
  retire = () => {
    this.live = false;
  };
  composerLayout = (height: number): boolean => {
    try {
      if (
        !this.live ||
        !this.port?.current() ||
        !Number.isFinite(height) ||
        height <= 0 ||
        height === this.composerHeight
      )
        return false;
      this.composerHeight = height;
      this.revision++;
      return true;
    } catch {
      return false;
    }
  };
  insetRequested = ({ top, bottom }: { top: number; bottom: number }): boolean => {
    try {
      if (
        !this.live ||
        !this.port?.current() ||
        ![top, bottom].every(Number.isFinite) ||
        top < 0 ||
        bottom < 0 ||
        (top === this.requestedInset?.top && bottom === this.requestedInset.bottom)
      )
        return false;
      this.requestedInset = { top, bottom };
      this.revision++;
      return true;
    } catch {
      return false;
    }
  };
  receive = (packet: ConversationNativeMetrics) => {
    try {
      this.record(packet);
    } catch {
      /* QA ports cannot affect chat event delivery. */
    }
  };
  private record(packet: ConversationNativeMetrics) {
    if (
      !this.live ||
      !this.port?.current() ||
      packet?.binding !== this.binding ||
      typeof packet?.sequence !== 'string' ||
      !/^(0|[1-9][0-9]{0,19})$/.test(packet?.sequence ?? '')
    )
      return;
    const sequence = BigInt(packet.sequence);
    const g = packet.geometry;
    if (
      sequence <= this.sequence ||
      sequence > 18446744073709551615n ||
      !g ||
      ![
        g.contentOffset?.y,
        g.contentSize?.height,
        g.layoutMeasurement?.height,
        g.contentInset?.top,
        g.contentInset?.bottom,
      ].every(Number.isFinite) ||
      g.contentSize.height <= 0 ||
      g.layoutMeasurement.height <= 0 ||
      (g.contentInset.top ?? -1) < 0 ||
      g.contentInset.bottom < 0
    )
      return;
    this.sequence = sequence;
    const c = this.port.context();
    if (![c.keyboardHeight, c.keyboardProgress].every(Number.isFinite)) return;
    this.searched ||= c.searchActive;
    const phase = c.searchActive ? 'search' : this.searched ? 'restored' : 'before-search';
    // Copy a closed schema, rather than serializing native events/context objects.
    const list =
      c.list && Object.values(c.list).every((v) => v === null || Number.isFinite(v))
        ? c.list
        : null;
    const record = {
      activation: this.activation,
      phase,
      searchActive: c.searchActive,
      reply: c.reply,
      editing: c.editing,
      keyboard: { height: c.keyboardHeight, progress: c.keyboardProgress },
      // JS layout and the SDK callback describe desired reservation, not a
      // UIKit acknowledgement. Keep them separate from the bound native facts.
      composer: { height: this.composerHeight },
      requestedInset: this.requestedInset ? { ...this.requestedInset } : null,
      native: {
        contentOffset: { y: g.contentOffset.y },
        contentSize: { height: g.contentSize.height },
        layoutMeasurement: { height: g.layoutMeasurement.height },
        contentInset: { top: g.contentInset.top, bottom: g.contentInset.bottom },
      },
      list: list
        ? {
            firstItemOffset: list.firstItemOffset,
            childHeight: list.childHeight,
            windowHeight: list.windowHeight,
            lastRowY: list.lastRowY,
            lastRowHeight: list.lastRowHeight,
          }
        : null,
    };
    const encoded = JSON.stringify(record);
    if (encoded === this.last || this.counts[phase] >= 32) return;
    this.last = encoded;
    this.counts[phase]++;
    try {
      this.port.emit({ ...record, sequence: packet.sequence });
    } catch {
      /* Optional diagnostics cannot affect chat. */
    }
  }
  async sample(read: (binding: string) => Promise<ConversationNativeMetrics | null>) {
    try {
      if (!this.live || !this.port?.current()) return;
      const revision = ++this.revision;
      const searching = this.port.context().searchActive;
      this.searched ||= searching;
      const packet = await read(this.binding);
      // A read captured before a phase or composer change cannot describe the
      // restored layout. Real later native notifications remain observable.
      if (revision !== this.revision || searching !== this.port?.context().searchActive) return;
      if (packet) this.receive(packet);
    } catch {
      /* A missing native view is an unavailable observation, not a scroll failure. */
    }
  }
}
