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

type RevealPort = Readonly<{
  current: (id: string) => boolean;
  item: (id: string) => ConversationRevealItem | null;
  frame: (work: () => void) => () => void;
  scroll: (offset: number) => void;
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
  private revision = 0;
  private cancellation = 0;
  private cancelFrame: (() => void) | null = null;
  constructor(scope: string, port?: RevealPort) {
    this.scope = scope;
    this.port = port ?? null;
  }
  connect(port: RevealPort) {
    this.port = port;
    this.schedule();
  }
  capture = () => this.cancellation;
  isCurrent = (captured: number) => captured === this.cancellation;
  request(id: string, captured = this.cancellation) {
    if (captured !== this.cancellation) return;
    this.cancel();
    this.target = id;
    this.schedule();
  }
  cancel = () => {
    ++this.cancellation;
    ++this.revision;
    this.target = null;
    this.cancelFrame?.();
    this.cancelFrame = null;
  };
  onScroll = (geometry: ConversationRevealGeometry) => {
    // User scrolling cancels this intent, but its geometry is still the basis
    // for the next explicit search or arrow press.
    this.geometry = geometry;
    if (geometry.responderIgnoreScroll === false) {
      this.cancel();
      return;
    }
    this.schedule();
  };
  onInset = ({ top, bottom }: { top: number; bottom: number }) => {
    if (!Number.isFinite(top) || !Number.isFinite(bottom)) return;
    this.inset = { top, bottom };
    this.schedule();
  };
  onContentSize = (_width: number, height: number) => {
    if (!Number.isFinite(height) || height <= 0) return;
    this.contentHeight = height;
    this.schedule();
  };
  onLayout = (height: number) => {
    if (!Number.isFinite(height) || height <= 0) return;
    this.viewportHeight = height;
    this.schedule();
  };
  onCommit = () => this.schedule();

  private ready(): boolean {
    const geometry = this.geometry;
    return Boolean(
      geometry &&
      this.inset &&
      this.contentHeight !== null &&
      this.viewportHeight !== null &&
      Math.abs((geometry.contentInset.top ?? 0) - this.inset.top) <= 1 &&
      Math.abs(geometry.contentInset.bottom - this.inset.bottom) <= 1 &&
      Math.abs(geometry.contentSize.height - this.contentHeight) <= 1 &&
      Math.abs(geometry.layoutMeasurement.height - this.viewportHeight) <= 1,
    );
  }
  private schedule() {
    this.cancelFrame?.();
    this.cancelFrame = null;
    const port = this.port;
    if (!port || !this.target || !this.ready()) return;
    const id = this.target;
    const revision = ++this.revision;
    this.cancelFrame = port.frame(() => {
      if (revision !== this.revision || this.target !== id) return;
      this.cancelFrame = null;
      if (!port.current(id)) {
        this.cancel();
        return;
      }
      const geometry = this.geometry;
      if (!this.ready() || !geometry) return;
      const item = port.item(id);
      if (!item) return; // A new timeline waits for its real FlashList commit.
      const offset = conversationRevealOffset(geometry, item);
      if (offset === null) return;
      this.target = null;
      if (Math.abs(offset - geometry.contentOffset.y) > 0.5) port.scroll(offset);
    });
  }
}
