type Port = Readonly<{
  current: () => boolean;
  frame: (work: () => void) => () => void;
  scroll: () => boolean;
}>;

type NativeScrollGeometry = Readonly<{
  responderIgnoreScroll?: boolean;
  contentOffset: { y: number };
  contentSize: { height: number };
  layoutMeasurement: { height: number };
  contentInset: { bottom: number };
}>;

/** Native content excludes its inset; reserve the composer once at the end. */
export function initialTimelineEndOffset({
  contentHeight,
  viewportHeight,
  bottomInset,
}: {
  contentHeight: number;
  viewportHeight: number;
  bottomInset: number;
}): number | null {
  if (
    !Number.isFinite(contentHeight) ||
    !Number.isFinite(viewportHeight) ||
    !Number.isFinite(bottomInset) ||
    contentHeight <= 0 ||
    viewportHeight <= 0 ||
    bottomInset <= 0
  )
    return null;
  const offset = contentHeight - viewportHeight + bottomInset;
  return Number.isFinite(offset) ? Math.max(0, offset) : null;
}

/** Keep the initial native end until the caller cancels for the first user intent. */
export class InitialTimelineEnd {
  readonly scope: string;
  private port: Port | undefined;
  private loaded = false;
  private footerReady = false;
  private contentReady = false;
  private retired = false;
  private revision = 0;
  private cancelFrame: (() => void) | undefined;
  private contentHeight = 0;
  private viewportHeight = 0;
  private bottomInset = 0;
  private nativeGeometry: NativeScrollGeometry | undefined;
  private readonly options: Readonly<{ anchorUntilInteraction?: boolean }>;

  constructor(
    scope: string,
    port?: Port,
    options: Readonly<{ anchorUntilInteraction?: boolean }> = {},
  ) {
    this.scope = scope;
    this.port = port;
    this.options = options;
  }

  connect(port: Port) {
    if (this.retired) return;
    this.port = port;
    this.schedule();
  }

  onLoad = () => {
    this.loaded = true;
    this.schedule();
  };

  onFooterLayout = (height: number) => {
    if (!Number.isFinite(height) || height <= 0) return;
    this.bottomInset = height;
    this.footerReady = true;
    this.schedule();
  };

  onContentSizeChange = (width: number, height: number) => {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return;
    this.contentHeight = height;
    this.contentReady = true;
    this.schedule();
  };

  onViewportLayout = (height: number) => {
    if (!Number.isFinite(height) || height <= 0) return;
    this.viewportHeight = height;
    this.schedule();
  };

  onNativeScroll = (geometry: NativeScrollGeometry) => {
    // Native user scrolling includes status-bar scroll-to-top, which does not
    // emit a drag event. An absent marker carries no user-intent signal.
    if (geometry.responderIgnoreScroll === false) {
      this.cancel();
      return;
    }
    if (
      !Number.isFinite(geometry.contentOffset.y) ||
      initialTimelineEndOffset({
        contentHeight: geometry.contentSize.height,
        viewportHeight: geometry.layoutMeasurement.height,
        bottomInset: geometry.contentInset.bottom,
      }) === null
    )
      return;
    this.nativeGeometry = geometry;
    this.schedule();
  };

  // FlashList calls this after its own initial-index layout work. Follow the
  // newest commit if one overtakes the pending frame, without a timing guess.
  onCommitLayoutEffect = () => {
    this.schedule();
  };

  cancel = () => {
    this.retired = true;
    this.revision++;
    this.cancelFrame?.();
    this.cancelFrame = undefined;
  };

  private schedule() {
    if (this.retired || !this.port || !this.loaded || !this.footerReady || !this.contentReady)
      return;
    this.cancelFrame?.();
    this.cancelFrame = undefined;
    const revision = ++this.revision;
    if (this.options.anchorUntilInteraction && this.isAtEnd()) return;
    this.cancelFrame = this.port.frame(() => {
      if (this.retired || revision !== this.revision) return;
      this.cancelFrame = undefined;
      if (!this.port?.current()) {
        this.cancel();
        return;
      }
      if (this.options.anchorUntilInteraction && this.isAtEnd()) return;
      // The native end includes content size and bottom inset, so this request
      // covers either a native composer reservation or the fallback footer.
      if (this.port.scroll() && !this.options.anchorUntilInteraction) this.cancel();
    });
  }

  private isAtEnd() {
    const geometry = this.nativeGeometry;
    if (
      !geometry ||
      Math.abs(geometry.contentSize.height - this.contentHeight) > 1 ||
      (this.viewportHeight > 0 &&
        Math.abs(geometry.layoutMeasurement.height - this.viewportHeight) > 1) ||
      Math.abs(geometry.contentInset.bottom - this.bottomInset) > 1
    )
      return false;
    const end = initialTimelineEndOffset({
      contentHeight: geometry.contentSize.height,
      viewportHeight: geometry.layoutMeasurement.height,
      bottomInset: geometry.contentInset.bottom,
    });
    return end !== null && Math.abs(geometry.contentOffset.y - end) <= 1;
  }
}
