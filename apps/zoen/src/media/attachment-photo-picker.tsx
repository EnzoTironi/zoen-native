import { pickAttachmentAssets } from './attachment-picker';
import type { PhotoPickerSubmission } from './attachment-photo-session';
const cancel = () => {};
const pick = (remaining: number, current: () => boolean) =>
  pickAttachmentAssets('photo', remaining, current);
/** Native embedded library is iOS-only; other platforms keep their existing SDK picker. */
export function useAttachmentPhotoPicker() {
  return {
    pick,
    cancel,
    view: null,
    preferredUri: (): string | null => null,
    submission: (): PhotoPickerSubmission | null => null,
  };
}
