import type { ConversationAttachmentCategory } from './attachment-categories';
import { pickAttachmentAssets } from './attachment-picker';
import type { PhotoPickerAdmission, PhotoPickerSubmission } from './attachment-photo-session';
import type { RecentAttachmentConfiguration } from './attachment-recent-session';
const cancel = () => {};
const pick = (remaining: number, current: () => boolean, _admission?: PhotoPickerAdmission) =>
  pickAttachmentAssets('photo', remaining, current);
/** Native embedded library is iOS-only; other platforms keep their existing SDK picker. */
export function useAttachmentPhotoPicker(
  _recentConfiguration?: RecentAttachmentConfiguration,
  _routes?: {
    categories: readonly ConversationAttachmentCategory[];
    open(category: ConversationAttachmentCategory, current: () => boolean): void;
  },
) {
  return {
    pick,
    cancel,
    view: null,
    preferredUri: (): string | null => null,
    submission: (): PhotoPickerSubmission | null => null,
  };
}
