import type { SpaceSummary } from '../core/generated/SpaceSummary';
import { t } from '../i18n/index.ts';

/** Never classify a self conversation or a user-chosen title as Saved Messages. */
export function conversationTitle(chat: Pick<SpaceSummary, 'kind' | 'title'>): string {
  return chat.kind === 'saved' ? t('settings.savedMessages') : chat.title;
}
