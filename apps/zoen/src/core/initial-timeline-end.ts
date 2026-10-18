type Port = Readonly<{
  current: () => boolean;
  frame: (work: () => void) => () => void;
  scroll: () => boolean;
}>;

/** Reconcile the first native end after the list and composer reservation are ready. */
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
      // The native end includes content size and bottom inset, so this request
      // covers either a native composer reservation or the fallback footer.
      if (this.port.scroll()) this.cancel();
    });
  }
}
