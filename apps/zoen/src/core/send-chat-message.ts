import { t } from '../i18n/index.ts';
import type { useMessageDraft } from './use-message-draft';
type Draft = Pick<
  ReturnType<typeof useMessageDraft>,
  'capture' | 'send' | 'sendEdit' | 'isCurrent'
>;
/** Existing draft/receipt authority owns delivery; the screen only owns busy feedback. */
export async function sendChatMessage({
  draft,
  scope,
  blocked,
  allowEmptyCaption,
  flight,
  setPending,
  typing,
  inform,
}: {
  draft: Draft;
  scope: string;
  blocked: boolean;
  allowEmptyCaption: boolean;
  flight: { current: { scope: string } | null };
  setPending(value: string | null): void;
  typing(): void;
  inform(title: string, detail: string): Promise<unknown>;
}) {
  if (blocked || flight.current?.scope === scope) return;
  const attempt = draft.capture();
  if (!attempt || (!attempt.text.trim() && !(attempt.editingId && allowEmptyCaption))) return;
  const token = { scope };
  flight.current = token;
  typing();
  setPending(scope);
  try {
    if (attempt.editingId) await draft.sendEdit(attempt);
    else await draft.send();
  } catch (reason) {
    if (flight.current === token && draft.isCurrent(attempt))
      void inform(
        t(attempt.editingId ? 'chat.editCouldNotBeSaved' : 'chat.theReplyCouldNotBeSent'),
        reason instanceof Error ? reason.message : t('chat.tryAgain'),
      );
  } finally {
    if (flight.current === token) {
      flight.current = null;
      setPending(null);
    }
  }
}
