import type { router } from 'expo-router';
import { homeSpacesFor } from '../home/access';
import { t } from '../i18n/index.ts';
import type { useChatController } from './use-chat-controller';

/** Keep cross-chat routing and local history reveal on their existing authorized boundaries. */
export function openChatQuote(
  {
    store,
    reveal,
    setScrollTarget,
    inform,
  }: Pick<ReturnType<typeof useChatController>, 'store' | 'reveal' | 'setScrollTarget' | 'inform'>,
  navigation: Pick<typeof router, 'push'>,
  messageId: string,
  sourceSpace: string | undefined,
  originId: string,
) {
  if (sourceSpace) {
    if (homeSpacesFor(store.getSnapshot()).has(sourceSpace))
      navigation.push({ pathname: '/chat/[id]', params: { id: sourceSpace, jump: messageId } });
    return;
  }
  void reveal(messageId)
    .then((target) => {
      if (target) setScrollTarget(target, originId);
    })
    .catch(() => void inform(t('chat.theMessageCouldNotBeOpened'), t('chat.tryAgainInAMoment')));
}
