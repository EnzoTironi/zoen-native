import { pickAttachmentAssets } from './attachment-picker';
import type { PhotoPickerSubmission } from './attachment-photo-session';
import type { RecentAttachmentConfiguration } from './attachment-recent-session';
const cancel = () => {};
const pick = (remaining: number, current: () => boolean) =>
  pickAttachmentAssets('photo', remaining, current);
/** Native embedded library is iOS-only; other platforms keep their existing SDK picker. */
export function useAttachmentPhotoPicker(_recentConfiguration?: RecentAttachmentConfiguration) {
  return {
    pick,
    cancel,
    view: null,
    preferredUri: (): string | null => null,
    submission: (): PhotoPickerSubmission | null => null,
  };
}
