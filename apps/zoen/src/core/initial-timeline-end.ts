type Port = Readonly<{
  current: () => boolean;
  frame: (work: () => void) => () => void;
  scroll: () => boolean;
}>;

/** Reconcile the first native end position after the list and its footer have laid out. */
export class InitialTimelineEnd {
  readonly scope: string;
  private port: Port | undefined;
  private loaded = false;
  private footerReady = false;
  private contentReady = false;
  private retired = false;
  private revision = 0;
  private cancelFrame: (() => void) | undefined;

  constructor(scope: string, port?: Port) {
    this.scope = scope;
    this.port = port;
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
    this.footerReady = true;
    this.schedule();
  };

  onContentSizeChange = (width: number, height: number) => {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return;
    this.contentReady = true;
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
    const revision = ++this.revision;
    this.cancelFrame = this.port.frame(() => {
      if (this.retired || revision !== this.revision) return;
      this.cancelFrame = undefined;
      if (!this.port?.current()) {
        this.cancel();
        return;
      }
      // Use native content size, including the footer, for this nonanimated
      // initial request rather than the list's item-container bounds.
      if (this.port.scroll()) this.cancel();
    });
  }
}
