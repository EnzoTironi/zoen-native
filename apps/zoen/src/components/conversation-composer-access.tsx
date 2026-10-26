import type { ReactNode } from 'react';
import { Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useClient } from '../core/provider';
import { homeOwnerFor, homeSpacesFor } from '../home/access';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';

/** Presentation only: the existing draft/native authorities still admit every write. */
export function ConversationComposerAccess({
  spaceId,
  searching = false,
  children,
}: {
  spaceId: string;
  searching?: boolean;
  children: ReactNode;
}) {
  const client = useClient();
  const { presentation: p } = useTheme();
  const { bottom } = useSafeAreaInsets();
  const owner = homeOwnerFor(client);
  const identity = client.snapshot?.me?.id ?? client.snapshot?.account?.identity_id;
  const conversation = client.snapshot?.conversations.find((space) => space.id === spaceId);
  const member = homeSpacesFor(client).has(spaceId);
  // An unavailable owner, unknown chat or pending load is not evidence of leaving.
  const readOnly = Boolean(
    !client.store.isSessionRetiring &&
    owner &&
    owner.identityId === identity &&
    conversation &&
    (!member || conversation.can_write === false),
  );
  if (searching || !readOnly) return children;
  return (
    <View
      testID="conversation-read-only"
      style={{ paddingHorizontal: 16, paddingTop: 12, paddingBottom: Math.max(bottom, 12) }}
    >
      <Text style={{ color: p.chat.service.text, fontSize: 13, textAlign: 'center' }}>
        {t(member ? 'chat.readOnlyChat' : 'chat.noLongerAMember')}
      </Text>
    </View>
  );
}
