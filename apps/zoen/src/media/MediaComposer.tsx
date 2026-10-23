import type { MediaComposerOptions } from './media-composer-contract';
import { renderMediaComposer } from './media-composer-view';
import { useMediaComposerController } from './use-media-composer-controller';

/** Explicit record → review → send. An interrupted recording is never posted. */
export function MediaComposer(options: MediaComposerOptions) {
  return renderMediaComposer(useMediaComposerController(options));
}
