import type {
  MessageContextConfiguration,
  MessageContextRequest,
  MessageContextSelection,
} from './message-context-ports';

type Attempt = Readonly<{ id: string; allowed: ReadonlySet<string> }>;
/** UI lease only. Canonical callbacks re-read account/access/current message again. */
export class MessageContextPresentation {
  private attempt: Attempt | null = null;
  private retired = false;
  private drain: Promise<void> = Promise.resolve();
  private readonly options: Readonly<{
    request: MessageContextRequest;
    current(): boolean;
    id(): string;
    selected(value: MessageContextSelection): void;
    dismissed?(): void;
  }>;
  constructor(options: MessageContextPresentation['options']) {
    this.options = options;
  }
  async open(config: MessageContextConfiguration): Promise<void> {
    if (this.retired || this.attempt || !this.current()) return;
    const configuration = JSON.stringify(config);
    const attempt = {
      id: this.options.id(),
      allowed: new Set([
        ...config.actions.map((action) => action.id),
        ...config.reactions.map((reaction) => `reaction:${reaction.emoji}`),
        ...(config.moreLabel ? ['more'] : []),
      ]),
    };
    this.attempt = attempt;
    try {
      await this.drain;
      if (!this.current() || this.attempt !== attempt) return;
      const request = this.options.request;
      const result = await request.native.presentContext(
        attempt.id,
        request.binding,
        request.gestureId,
        configuration,
      );
      if (!this.current() || this.attempt !== attempt) return;
      this.attempt = null;
      const quote =
        attempt.allowed.has('select') && result?.startsWith('select:') && result.length <= 65543;
      if (result === null || (!attempt.allowed.has(result) && !quote)) {
        this.options.dismissed?.();
        return;
      }
      // The native promise is the ACTUAL host-restored/controller-removed barrier.
      this.options.selected(result as MessageContextSelection);
    } catch (reason) {
      await this.close();
      throw reason;
    } finally {
      if (this.attempt === attempt) this.attempt = null;
    }
  }
  current(): boolean {
    return !this.retired && this.options.request.current() && this.options.current();
  }
  close(): Promise<void> {
    const attempt = this.attempt;
    this.attempt = null;
    if (!attempt) return this.drain;
    const previous = this.drain;
    let removal: Promise<void>;
    try {
      removal = this.options.request.native.dismissContext(attempt.id);
    } catch (error) {
      removal = Promise.reject(error);
    }
    this.drain = Promise.all([previous, removal]).then(() => {});
    return this.drain;
  }
  retire(): Promise<void> {
    this.retired = true;
    return this.close();
  }
}
