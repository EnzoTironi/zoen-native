import {
  mediaPlayback,
  type PlaybackCoordinator,
  type PlaybackIntent,
  type PlaybackOwner,
} from './playback-owner.ts';
import type { MediaDraftScope } from './media-draft-types.ts';

export type AttachmentReviewRequest = Readonly<{
  scope: MediaDraftScope;
  revision: string;
  assetId: string;
  mutationId: string;
  sha256: string;
  replyTo: string | null;
}>;
export type AttachmentReviewState = Readonly<{
  mutationId: string | null;
  error: Error | null;
}>;
export type AttachmentReviewPort = {
  prepare(mutationId: string): Promise<AttachmentReviewRequest | null>;
  current(request: AttachmentReviewRequest): boolean;
  token(): string;
  preview(request: AttachmentReviewRequest, token: string): Promise<void>;
  cancel(token: string): Promise<void>;
};
type Attempt = {
  token: string;
  owner: PlaybackOwner;
  intent: PlaybackIntent | null;
  closed: boolean;
  started: boolean;
  request: AttachmentReviewRequest | null;
  work: Promise<void>;
  cancellation: Promise<void> | null;
};

/** A native read-only presentation, never an owner of draft bytes or a send intent. */
export class AttachmentReview {
  private readonly port: AttachmentReviewPort;
  private readonly coordinator: PlaybackCoordinator;
  private active: Attempt | null = null;
  private state: AttachmentReviewState = Object.freeze({ mutationId: null, error: null });
  private listeners = new Set<() => void>();

  constructor(port: AttachmentReviewPort, coordinator: PlaybackCoordinator = mediaPlayback) {
    this.port = port;
    this.coordinator = coordinator;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private publish(mutationId: string | null, error: Error | null = null) {
    this.state = Object.freeze({ mutationId, error });
    for (const listener of this.listeners) listener();
  }
  open = (mutationId: string): Promise<void> => {
    if (this.active) return this.active.work;
    const attempt: Attempt = {
      token: this.port.token(),
      owner: null!,
      intent: null,
      closed: false,
      started: false,
      request: null,
      work: Promise.resolve(),
      cancellation: null,
    };
    attempt.owner = this.coordinator.register(
      () => {
        if (this.active === attempt) void this.close().catch(() => {});
      },
      () => false,
    );
    this.active = attempt;
    attempt.intent = attempt.owner.begin();
    if (!attempt.intent) attempt.closed = true;
    this.publish(mutationId);
    // Admission is deferred so close can revoke even a not-yet-started prepare.
    attempt.work = Promise.resolve().then(async () => {
      try {
        if (attempt.closed || !attempt.owner.isCurrent(attempt.intent)) return;
        const request = await this.port.prepare(mutationId);
        if (
          !request ||
          attempt.closed ||
          !attempt.owner.isCurrent(attempt.intent) ||
          !this.port.current(request)
        )
          return;
        attempt.request = request;
        attempt.started = true;
        await this.port.preview(request, attempt.token);
      } catch (reason) {
        if (!attempt.closed && this.active === attempt)
          this.publish(mutationId, reason instanceof Error ? reason : new Error('Preview failed'));
      } finally {
        if (this.active === attempt) {
          this.active = null;
          this.publish(null, attempt.closed ? null : this.state.error);
        }
        attempt.owner.dispose();
      }
    });
    return attempt.work;
  };
  /** Revoke admission immediately; wait for native dismissal before callers release copies. */
  close = (): Promise<void> => {
    const attempt = this.active;
    if (!attempt) return Promise.resolve();
    attempt.closed = true;
    attempt.owner.release(attempt.intent);
    if (!attempt.started) return attempt.work;
    if (!attempt.cancellation) {
      const cancellation = Promise.resolve().then(() => this.port.cancel(attempt.token));
      attempt.cancellation = cancellation;
      void cancellation.catch(() => {
        if (attempt.cancellation === cancellation) attempt.cancellation = null;
      });
    }
    return Promise.all([attempt.cancellation, attempt.work]).then(() => {});
  };
  refresh = () => {
    const attempt = this.active;
    if (attempt?.request && !this.port.current(attempt.request))
      void this.close().catch(() => {
        // Cleanup retains this controller and retries through close; never reopen a stale URI.
      });
  };
}
