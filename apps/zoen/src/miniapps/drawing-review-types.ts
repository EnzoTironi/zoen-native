import type { SceneToolReview } from '../core/generated/SceneToolReview';
export type DrawingReviewImage = Readonly<{
  uri: string;
  width: number;
  height: number;
  sha256: string;
}>;
export type DrawingReviewPreview = Readonly<{
  review: SceneToolReview;
  before: DrawingReviewImage;
  after: DrawingReviewImage;
  key: string;
}>;
/** A focused admitted frame, not another renderer or approval authority. */
export type DrawingReviewPort = Readonly<{
  key: string;
  subscribe: (listener: () => void) => () => void;
  getSnapshot: () => boolean;
  render: (requestId: string, contentHash: string) => Promise<DrawingReviewPreview>;
}>;
