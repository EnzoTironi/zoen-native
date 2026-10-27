import { Stack, router } from 'expo-router';
import { t } from '../../i18n';
import { useDialogs } from '../../components/dialogs';
import { useMutedChats } from '../../notifications/use-muted-chats';
import { MutedChatsScreen, mutedChatName } from '../../notifications/muted-chats-screen';

export default function MutedChats() {
  const { state, session, accountRequired } = useMutedChats();
  const { confirm } = useDialogs();
  const ticket = session.ticket(state);
  return (
    <>
      <Stack.Screen options={{ title: t('notifications.mutedChats') }} />
      <MutedChatsScreen
        state={state}
        session={session}
        ticket={ticket}
        accountRequired={accountRequired}
        onOpen={(chat) => {
          if (session.canOpen(chat.id, ticket))
            router.push({ pathname: '/chat/[id]', params: { id: chat.id } });
        }}
        onUnmute={(chat) => {
          void session.unmute(
            chat.id,
            () =>
              confirm({
                title: t('notifications.reenable'),
                message: t('notifications.reenableNamedChat', { name: mutedChatName(chat) }),
                confirmLabel: t('notifications.reenable'),
              }),
            ticket,
          );
        }}
      />
    </>
  );
}
