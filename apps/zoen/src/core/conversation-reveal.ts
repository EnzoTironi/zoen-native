export type ConversationRevealGeometry = Readonly<{
  contentOffset: { y: number };
  contentSize: { height: number };
  layoutMeasurement: { height: number };
  contentInset: { top?: number; bottom: number };
  responderIgnoreScroll?: boolean;
}>;
export type ConversationRevealItem = Readonly<{
  y: number;
  height: number;
  measured: boolean;
  firstItemOffset: number;
  childHeight: number;
  windowHeight: number;
  topObstruction: number;
  bottomAligned: boolean;
}>;

/** FlashList's physical viewport and row layout exclude native keyboard insets.
 * Its short-content bottom alignment is a separate visual container margin. */
export function conversationRevealOffset(
  geometry: ConversationRevealGeometry,
  item: ConversationRevealItem,
): number | null {
  const viewport = geometry.layoutMeasurement.height;
  const content = geometry.contentSize.height;
  const bottom = geometry.contentInset.bottom;
  const top = geometry.contentInset.top ?? 0;
  if (
    ![
      viewport,
      content,
      bottom,
      top,
      geometry.contentOffset.y,
      item.y,
      item.height,
      item.firstItemOffset,
      item.childHeight,
      item.windowHeight,
      item.topObstruction,
    ].every(Number.isFinite) ||
    viewport <= 0 ||
    content <= 0 ||
    bottom < 0 ||
    top < 0 ||
    item.height <= 0 ||
    item.childHeight <= 0 ||
    item.windowHeight <= 0
  )
    return null;
  const visibleTop = Math.max(0, item.topObstruction);
  const visibleBottom = viewport - bottom;
  if (visibleBottom <= visibleTop) return null;
  const alignment = item.bottomAligned
    ? Math.max(0, item.windowHeight - item.childHeight - item.firstItemOffset)
    : 0;
  const rowTop = item.y + item.firstItemOffset + alignment;
  const current = geometry.contentOffset.y;
  const min = -top;
  const max = Math.max(min, content - viewport + bottom);
  const clamp = (offset: number) => Math.min(max, Math.max(min, offset));
  // Searching an already visible message must not reset a short chat to its
  // closed-keyboard origin. Leave the user's native offset alone.
  if (rowTop - current >= visibleTop && rowTop + item.height - current <= visibleBottom)
    return clamp(current);
  return clamp(rowTop - visibleTop);
}

function conversationRevealVisible(
  geometry: ConversationRevealGeometry,
  item: ConversationRevealItem,
): boolean {
  const top = Math.max(0, item.topObstruction);
  const bottom = geometry.layoutMeasurement.height - geometry.contentInset.bottom;
  const alignment = item.bottomAligned
    ? Math.max(0, item.windowHeight - item.childHeight - item.firstItemOffset)
    : 0;
  const start = item.y + item.firstItemOffset + alignment - geometry.contentOffset.y;
  if (bottom <= top) return false;
  // A message taller than the usable viewport reveals its beginning. A normal
  // row must actually fit; equality with a clamped scroll limit proves neither.
  return item.height > bottom - top
    ? Math.abs(start - top) <= 0.5
    : start >= top - 0.5 && start + item.height <= bottom + 0.5;
}

type RevealPort = Readonly<{
  current: (id: string) => boolean;
  item: (id: string) => ConversationRevealItem | null;
  frame: (work: () => void) => () => void;
  scroll: (offset: number, animated: boolean) => void;
  trace?: (record: Readonly<Record<string, unknown>>) => void;
}>;

/** One explicit search request, committed against the latest measured native
 * inset/content/layout. A changed query, route or user drag cancels pending work. */
export class ConversationReveal {
  readonly scope: string;
  private port: RevealPort | null;
  private geometry: ConversationRevealGeometry | null = null;
  private inset: { top: number; bottom: number } | null = null;
  private contentHeight: number | null = null;
  private viewportHeight: number | null = null;
  private target: string | null = null;
  private data: readonly unknown[] | null = null;
  private committedData: readonly unknown[] | null = null;
  private commit = 0;
  private engagementCommit: number | null = null;
  private commandLayout: string | null = null;
  private revision = 0;
  private cancellation = 0;
  private cancelFrame: (() => void) | null = null;
  private traceData = new WeakMap<readonly unknown[], number>();
  private traceDataSequence = 0;
  constructor(scope: string, port?: RevealPort) {
    this.scope = scope;
    this.port = port ?? null;
  }
  connect(port: RevealPort) {
    this.port = port;
    this.trace('connect');
    this.schedule();
  }
  capture = () => this.cancellation;
  isCurrent = (captured: number) => captured === this.cancellation;
  request(id: string, captured = this.cancellation) {
    if (captured !== this.cancellation) {
      this.trace('request-rejected', { captured });
      return;
    }
    this.cancel();
    this.target = id;
    this.trace('request');
    this.schedule();
  }
  cancel = (reason = 'explicit') => {
    this.trace('cancel', { reason });
    ++this.cancellation;
    ++this.revision;
    this.target = null;
    this.commandLayout = null;
    this.engagementCommit = null;
    this.cancelFrame?.();
    this.cancelFrame = null;
  };
  onScroll = (geometry: ConversationRevealGeometry) => {
    // User scrolling cancels this intent, but its geometry is still the basis
    // for the next explicit search or arrow press.
    this.geometry = geometry;
    this.trace('scroll');
    if (geometry.responderIgnoreScroll === false) {
      this.cancel('native-user-scroll');
      return;
    }
    this.schedule();
  };
  onInset = ({ top, bottom }: { top: number; bottom: number }) => {
    if (!Number.isFinite(top) || !Number.isFinite(bottom)) return;
    this.inset = { top, bottom };
    this.trace('inset');
    this.schedule();
  };
  onContentSize = (_width: number, height: number) => {
    if (!Number.isFinite(height) || height <= 0) return;
    this.contentHeight = height;
    this.trace('content-size');
    this.schedule();
  };
  onLayout = (height: number) => {
    if (!Number.isFinite(height) || height <= 0) return;
    this.viewportHeight = height;
    this.trace('viewport');
    this.schedule();
  };
  onData = (data: readonly unknown[]) => {
    if (this.data !== data) {
      this.data = data;
      this.commandLayout = null;
      this.engagementCommit = null;
      this.trace('data');
    }
    this.schedule();
  };
  onCommit = (data: readonly unknown[]) => {
    this.committedData = data;
    ++this.commit;
    this.trace('commit');
    this.schedule();
  };

  private ready(): boolean {
    const geometry = this.geometry;
    return Boolean(
      geometry &&
      this.data !== null &&
      this.data === this.committedData &&
      this.inset &&
      this.contentHeight !== null &&
      this.viewportHeight !== null &&
      Math.abs((geometry.contentInset.top ?? 0) - this.inset.top) <= 1 &&
      Math.abs(geometry.contentInset.bottom - this.inset.bottom) <= 1 &&
      Math.abs(geometry.contentSize.height - this.contentHeight) <= 1 &&
      Math.abs(geometry.layoutMeasurement.height - this.viewportHeight) <= 1,
    );
  }
  private trace(event: string, detail: Readonly<Record<string, unknown>> = {}) {
    const trace = this.port?.trace;
    if (!trace) return;
    const dataId = (data: readonly unknown[] | null) => {
      if (!data) return null;
      let id = this.traceData.get(data);
      if (id === undefined) {
        id = ++this.traceDataSequence;
        this.traceData.set(data, id);
      }
      return id;
    };
    // Diagnostic records contain geometry and a short target fingerprint only;
    // scope embeds account ownership and must never be included in native logs.
    let hash = 0;
    if (this.target)
      for (let index = 0; index < this.target.length; index++)
        hash = (Math.imul(hash, 31) + this.target.charCodeAt(index)) | 0;
    const native = this.geometry;
    const waiting: string[] = [];
    if (!native) waiting.push('native');
    if (this.data === null || this.data !== this.committedData) waiting.push('data-commit');
    if (!this.inset) waiting.push('inset');
    if (this.contentHeight === null) waiting.push('content-size');
    if (this.viewportHeight === null) waiting.push('viewport');
    if (native && this.inset) {
      if (Math.abs((native.contentInset.top ?? 0) - this.inset.top) > 1)
        waiting.push('native-top-inset');
      if (Math.abs(native.contentInset.bottom - this.inset.bottom) > 1)
        waiting.push('native-bottom-inset');
    }
    if (native && Math.abs(native.contentSize.height - (this.contentHeight ?? NaN)) > 1)
      waiting.push('native-content-size');
    if (native && Math.abs(native.layoutMeasurement.height - (this.viewportHeight ?? NaN)) > 1)
      waiting.push('native-viewport');
    try {
      trace({
        event,
        target: this.target ? (hash >>> 0).toString(16).padStart(8, '0') : null,
        intent: this.cancellation,
        frame: this.revision,
        data: dataId(this.data),
        committedData: dataId(this.committedData),
        entries: this.data?.length ?? null,
        commit: this.commit,
        engagementCommit: this.engagementCommit,
        ready: this.ready(),
        waiting,
        native: native
          ? {
              contentOffset: { y: native.contentOffset.y },
              contentSize: { height: native.contentSize.height },
              layoutMeasurement: { height: native.layoutMeasurement.height },
              contentInset: {
                top: native.contentInset.top ?? 0,
                bottom: native.contentInset.bottom,
              },
              responderIgnoreScroll: native.responderIgnoreScroll,
            }
          : null,
        expected: {
          inset: this.inset,
          contentHeight: this.contentHeight,
          viewportHeight: this.viewportHeight,
        },
        ...detail,
      });
    } catch {
      // Optional diagnostics must not affect search or scrolling.
    }
  }
  private schedule() {
    this.cancelFrame?.();
    this.cancelFrame = null;
    const port = this.port;
    if (!port || !this.target || !this.ready()) {
      if (this.target) this.trace('wait');
      return;
    }
    const id = this.target;
    const revision = ++this.revision;
    this.cancelFrame = port.frame(() => {
      if (revision !== this.revision || this.target !== id) return;
      this.cancelFrame = null;
      if (!port.current(id)) {
        this.cancel('not-current');
        return;
      }
      const geometry = this.geometry;
      if (!this.ready() || !geometry) {
        this.trace('wait-frame');
        return;
      }
      const item = port.item(id);
      if (!item) {
        this.trace('wait-item');
        return; // A new timeline waits for its real FlashList commit.
      }
      const offset = conversationRevealOffset(geometry, item);
      if (offset === null) {
        this.trace('wait-offset', { item });
        return;
      }
      const atTarget = Math.abs(offset - geometry.contentOffset.y) <= 0.5;
      const visible = conversationRevealVisible(geometry, item);
      this.trace('sample', { item, offset, atTarget, visible });
      // A seek must engage and measure the target in a later SDK commit before
      // its native acknowledgement can finish the request. Index layouts may
      // still contain measured dimensions belonging to pre-pagination rows.
      if (
        item.measured &&
        atTarget &&
        visible &&
        (this.engagementCommit === null || this.commit > this.engagementCommit)
      ) {
        this.trace('retire');
        this.target = null;
        this.commandLayout = null;
        this.engagementCommit = null;
        return;
      }
      // Native animation frames are acknowledgements, not new seek requests.
      // Refine only after the actual committed layout/insets have changed.
      const layout = [
        item.y,
        item.height,
        item.measured,
        item.firstItemOffset,
        item.childHeight,
        item.windowHeight,
        item.topObstruction,
        item.bottomAligned,
        geometry.contentSize.height,
        geometry.layoutMeasurement.height,
        geometry.contentInset.top ?? 0,
        geometry.contentInset.bottom,
      ].join(':');
      if (atTarget || this.commandLayout === layout) {
        this.trace('wait-ack', {
          atTarget,
          visible,
          sameLayout: this.commandLayout === layout,
        });
        return;
      }
      this.engagementCommit ??= this.commit;
      this.commandLayout = layout;
      this.trace('issue', { offset, animated: item.measured });
      port.scroll(offset, item.measured);
    });
  }
}
