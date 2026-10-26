import type { DrawingReviewPort } from './drawing-review-types';
/** Navigation awaits the current document's acknowledged save before retiring its frame. */
export type MiniappDocumentController = Readonly<{
  /** Captured admitted UI pin; absent for the existing binary-editor port. */
  agentDocument?: Readonly<{ kind: 'owned-scene'; resourceUri: string }>;
  agentReadiness?: Readonly<{
    subscribe: (listener: () => void) => () => void;
    getSnapshot: () => boolean;
  }>;
  drawingReview?: DrawingReviewPort;
  /** Present only for an editor with qualified same-Session external refresh. */
  observedHead?: () => Promise<Readonly<{ itemId: string; version: number; sha256: string }>>;
  flush: (options?: { retire?: boolean }) => Promise<void>;
}>;
