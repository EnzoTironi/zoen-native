import { useEffect } from 'react';
import { ActivityIndicator, FlatList, Pressable, Text, View } from 'react-native';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { Avatar, EmptyState, NativeButton } from '../components/primitives';
import { useProfilePhoto } from './use-profile-photo';
import type {
  BlockedPeopleSession,
  BlockedPeopleState,
  BlockedPeopleTicket,
  BlockedPerson,
} from './blocked-people';

export const blockedPersonName = (person: BlockedPerson) =>
  person.profile?.name ||
  (person.profile?.handle ? `@${person.profile.handle}` : t('settings.blockedPerson'));

function PersonRow({
  person,
  disabled,
  pending,
  session,
  ticket,
  onOpen,
  onUnblock,
  first,
  last,
}: {
  person: BlockedPerson;
  disabled: boolean;
  pending: boolean;
  session: BlockedPeopleSession;
  ticket: BlockedPeopleTicket | null;
  onOpen: (id: string) => void;
  onUnblock: (person: BlockedPerson) => void;
  first: boolean;
  last: boolean;
}) {
  const { presentation: p } = useTheme();
  const profile = person.profile;
  const photo = useProfilePhoto(person.id, profile?.photo_sha256, profile?.photo_ready ?? false);
  const name = blockedPersonName(person);
  useEffect(() => {
    if (person.phase === 'idle') void session.loadProfile(person.id, ticket);
  }, [session, person.id, person.phase, ticket]);
  return (
    <View
      style={{
        backgroundColor: p.list.block,
        flexDirection: 'row',
        flexWrap: 'wrap',
        alignItems: 'center',
        borderTopLeftRadius: first ? 10 : 0,
        borderTopRightRadius: first ? 10 : 0,
        borderBottomLeftRadius: last ? 10 : 0,
        borderBottomRightRadius: last ? 10 : 0,
        overflow: 'hidden',
      }}
    >
      <Pressable
        testID={`blocked-person-${person.id}`}
        accessibilityRole="button"
        accessibilityLabel={name}
        accessibilityHint={
          person.phase === 'failed'
            ? t('settings.retryBlockedProfile')
            : t('settings.openBlockedProfile')
        }
        accessibilityState={{ disabled: disabled || person.phase === 'loading' }}
        disabled={disabled || person.phase === 'loading'}
        onPress={() =>
          person.phase === 'ready' ? onOpen(person.id) : void session.loadProfile(person.id, ticket)
        }
        style={({ pressed }) => ({
          flex: 1,
          flexBasis: 200,
          minHeight: 64,
          flexDirection: 'row',
          alignItems: 'center',
          gap: 12,
          paddingHorizontal: 16,
          paddingVertical: 10,
          backgroundColor: pressed ? p.list.highlighted : 'transparent',
        })}
      >
        <Avatar name={name} uri={photo} color={profile?.tint_hex} size={40} />
        <View style={{ flex: 1, gap: 3 }}>
          <Text style={{ color: p.list.text, fontSize: 17 }}>{name}</Text>
          {profile?.handle ? (
            <Text style={{ color: p.list.secondary, fontSize: 13 }}>@{profile.handle}</Text>
          ) : person.phase === 'failed' ? (
            <Text style={{ color: p.list.secondary, fontSize: 13 }}>
              {t('settings.blockedProfileUnavailable')}
            </Text>
          ) : null}
        </View>
      </Pressable>
      <Pressable
        testID={`unblock-person-${person.id}`}
        accessibilityRole="button"
        accessibilityLabel={t('settings.unblockNamedPerson', { name })}
        accessibilityState={{ disabled, busy: pending }}
        disabled={disabled}
        onPress={() => onUnblock(person)}
        style={({ pressed }) => ({
          minHeight: 44,
          minWidth: 44,
          marginLeft: 'auto',
          paddingHorizontal: 16,
          justifyContent: 'center',
          opacity: disabled ? 0.4 : pressed ? 0.5 : 1,
        })}
      >
        {pending ? (
          <ActivityIndicator color={p.list.accent} />
        ) : (
          <Text style={{ color: p.list.accent, fontSize: 17 }}>{t('chat.unblock')}</Text>
        )}
      </Pressable>
    </View>
  );
}

export function BlockedPeopleView({
  state,
  session,
  ticket,
  accountRequired,
  onOpen,
  onUnblock,
}: {
  state: BlockedPeopleState;
  session: BlockedPeopleSession;
  ticket: BlockedPeopleTicket | null;
  accountRequired: boolean;
  onOpen: (id: string) => void;
  onUnblock: (person: BlockedPerson) => void;
}) {
  const { presentation: p } = useTheme();
  const disabled = state.phase !== 'ready' || state.pending !== null;
  return (
    <FlatList
      testID="blocked-people-screen"
      data={accountRequired ? [] : state.people}
      keyExtractor={(person) => person.id}
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 16,
        paddingBottom: 48,
        maxWidth: 640,
        width: '100%',
        alignSelf: 'center',
      }}
      ListHeaderComponent={
        <View style={{ gap: 12, paddingBottom: 20 }}>
          <Text style={{ color: p.list.secondary, fontSize: 15, lineHeight: 21 }}>
            {t('settings.blockedPeopleDescription')}
          </Text>
          {state.readFailed ? (
            <Text accessibilityRole="alert" style={{ color: p.destructive }}>
              {t('settings.blockedPeopleReadFailed')}
            </Text>
          ) : null}
          {state.actionFailed ? (
            <Text accessibilityRole="alert" style={{ color: p.destructive }}>
              {t('settings.blockedPeopleUnblockFailed')}
            </Text>
          ) : null}
        </View>
      }
      ListEmptyComponent={
        accountRequired ? (
          <EmptyState
            title={t('settings.yourAccount')}
            detail={t('settings.blockedPeopleAccountRequired')}
          />
        ) : state.phase === 'loading' || state.phase === 'inactive' ? (
          <ActivityIndicator accessibilityLabel={t('settings.pleaseWait')} color={p.list.accent} />
        ) : state.phase === 'ready' ? (
          <EmptyState
            title={t('settings.noBlockedPeople')}
            detail={t('settings.noBlockedPeopleDetail')}
          />
        ) : undefined
      }
      ListFooterComponent={
        !accountRequired ? (
          <View style={{ paddingTop: 20 }}>
            <NativeButton
              testID="blocked-people-refresh"
              label={state.readFailed ? t('chat.tryAgain2') : t('settings.refreshBlockedPeople')}
              secondary
              disabled={
                state.pending !== null || state.phase === 'loading' || state.phase === 'inactive'
              }
              onPress={() => void session.load(ticket)}
            />
          </View>
        ) : undefined
      }
      renderItem={({ item, index }) => (
        <PersonRow
          person={item}
          session={session}
          ticket={ticket}
          disabled={disabled}
          pending={state.pending === item.id}
          onOpen={onOpen}
          onUnblock={onUnblock}
          first={index === 0}
          last={index === state.people.length - 1}
        />
      )}
      ItemSeparatorComponent={() => (
        <View style={{ height: 1, backgroundColor: p.list.separator, marginLeft: 68 }} />
      )}
    />
  );
}
