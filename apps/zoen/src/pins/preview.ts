import { useSemanticMessagePreview } from '../components/use-semantic-message-preview';
import { messageMayContainMarkdown } from '../core/message-markdown';
import { messageText } from '../core/presentation';
import type { TimelineEntry } from '../core/types';
import { t } from '../i18n';
/** The maintained native parser owns Markdown; filenames and generated media labels are literal. */
export function usePinnedText(space: string, entry?: TimelineEntry): string {
  const authored = entry && 'Message' in entry.kind ? entry.kind.Message.text : '';
  const card = entry && 'Message' in entry.kind ? entry.kind.Message.card : null;
  const text = entry?.voice
    ? t('media.voiceMessage')
    : authored || card?.title || (entry ? messageText(entry) || t('chat.message') : '');
  return (
    useSemanticMessagePreview({
      owner: `pins:${entry?.id ?? ''}:${entry?.text_revision ?? ''}`,
      text,
      literal: !!entry?.voice || !authored || !messageMayContainMarkdown(text, true),
      enabled: !!entry,
      target: entry ? { spaceId: space, messageId: entry.id } : undefined,
    }).text ?? '…'
  );
}
