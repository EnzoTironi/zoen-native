import { Stack, router } from 'expo-router';
import { t } from '../../i18n';
import { useDialogs } from '../../components/dialogs';
import { useBlockedPeople } from '../../profile/use-blocked-people';
import { BlockedPeopleView, blockedPersonName } from '../../profile/blocked-people-view';

export default function BlockedPeople() {
  const { session, state, accountRequired } = useBlockedPeople();
  const { confirm } = useDialogs();
  const ticket = session.ticket(state);
  return (
    <>
      <Stack.Screen options={{ title: t('settings.blockedPeople') }} />
      <BlockedPeopleView
        state={state}
        session={session}
        ticket={ticket}
        accountRequired={accountRequired}
        onOpen={(id) => {
          if (session.canOpen(id, ticket))
            router.push({ pathname: '/person/[id]', params: { id } });
        }}
        onUnblock={(person) => {
          void session.unblock(
            person.id,
            () =>
              confirm({
                title: t('chat.unblockThisPerson'),
                message: t('settings.unblockNamedPersonMessage', {
                  name: blockedPersonName(person),
                }),
                confirmLabel: t('chat.unblock'),
              }),
            ticket,
          );
        }}
      />
    </>
  );
}
