import { t } from '../../i18n/index.ts';
import { useMutedChats } from '../../notifications/use-muted-chats';
import { MuteDurationSheet } from '../../notifications/mute-duration-sheet';
import { IntegrityWarning } from '../../integrity/integrity-warning';
import { ProfileAvatar } from '../../profile/profile-avatar';
import { ProfileHeader } from '../../profile/profile-header';
import { ProfileInfoRow, ProfileSection } from '../../profile/profile-section';
import { peerInfo, profileTitleCollapseOffset } from '../../profile/telegram-profile-presentation';
import { useProfilePhoto } from '../../profile/use-profile-photo';
import { useCallback, useRef, useState } from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { Host, Column, Switch } from '@expo/ui';
import { font, foregroundStyle, frame, padding } from '@expo/ui/swift-ui/modifiers';
import { Pressable, ScrollView, Share, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useDialogs } from '../../components/dialogs';
import { useClient } from '../../core/provider';
import { EmptyState } from '../../components/primitives';
import { useMemberActions } from '../../communities/use-member-actions';
import { MemberActionsSheet } from '../../communities/member-actions-sheet';
import { MemberAdditionSheet } from '../../communities/member-addition-sheet';
import { useTheme } from '../../theme';

export default function Space() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { snapshot, store } = useClient();
  const members = useMemberActions(id);
  const mute = useMutedChats(id);
  const muteTicket = mute.session.ticket(mute.state);
  const muteChat = mute.state.chats.find((item) => item.id === id) ?? null;
  const openMute = () => mute.session.select(id, muteTicket);
  const { presentation: p, scheme } = useTheme();
  const insets = useSafeAreaInsets();
  const { confirm } = useDialogs();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(false);
  const pending = useRef(false);
  const active = useRef(false);
  const owner = useRef({
    identity: snapshot?.account?.identity_id,
    device: snapshot?.account?.device_id,
    epoch: store.sessionEpoch,
    id,
  });
  useFocusEffect(
    useCallback(() => {
      active.current = true;
      return () => {
        active.current = false;
      };
    }, []),
  );
  function currentOwner() {
    const state = store.getSnapshot();
    return (
      active.current &&
      owner.current.id === id &&
      owner.current.epoch === store.sessionEpoch &&
      owner.current.identity === state.snapshot?.account?.identity_id &&
      owner.current.device === state.snapshot?.account?.device_id
    );
  }
  const chat = snapshot?.conversations.find((item) => item.id === id);
  const photo = useProfilePhoto(
    chat?.counterpart?.id,
    chat?.counterpart?.photo_sha256,
    chat?.counterpart?.photo_ready ?? false,
  );
  if (!chat)
    return (
      <EmptyState
        title={t('chat.chatUnavailable')}
        detail={t('chat.thisChatIsNoLongerOnThisDevice')}
      />
    );
  const muted = mute.accountRequired ? chat.muted : Boolean(muteChat?.muted_until_ms);
  const muteDisabled =
    busy ||
    (!mute.accountRequired &&
      (mute.state.phase !== 'ready' || mute.state.pending !== null || !muteChat));
  const selected = chat;
  const subtitle =
    chat.kind === 'Direct'
      ? chat.counterpart?.handle
        ? `@${chat.counterpart.handle}`
        : undefined
      : t('chat.participantCount', { count: chat.members.length });
  async function perform(action: () => Promise<unknown>) {
    if (pending.current || !currentOwner()) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (failure) {
      if (currentOwner())
        setError(failure instanceof Error ? failure.message : t('chat.thisCouldNotBeCompleted'));
    } finally {
      pending.current = false;
      if (currentOwner()) setBusy(false);
    }
  }
  function preference(key: 'pinned' | 'muted' | 'archived', value: boolean) {
    void perform(() => {
      const current = store.getSnapshot().snapshot?.conversations.find((item) => item.id === id);
      if (!current) throw new Error(t('chat.chatUnavailable'));
      return store.request({
        op: 'set_chat_prefs',
        space_id: id,
        pinned: current.pinned,
        muted: current.muted,
        ...(key !== 'muted' ? { preserve_mute: true } : {}),
        archived: current.archived,
        [key]: value,
      });
    });
  }
  const privacy =
    chat.privacy === 'EndToEnd'
      ? t('chat.endToEndEncryption')
      : chat.privacy === 'Closed'
        ? t('chat.closedChatReadableByTheServer')
        : t('chat.publicChat');
  const preferenceModifiers =
    process.env.EXPO_OS === 'ios'
      ? [
          frame({ minHeight: peerInfo.row, maxWidth: Infinity, alignment: 'leading' }),
          padding({ horizontal: peerInfo.edge }),
          font({ textStyle: 'body' }),
          foregroundStyle(p.list.text),
        ]
      : undefined;
  return (
    <>
      <ScrollView
        testID="conversation-details-screen"
        contentInsetAdjustmentBehavior="never"
        scrollEventThrottle={16}
        onScroll={(event) =>
          setCollapsed(
            event.nativeEvent.contentOffset.y > profileTitleCollapseOffset(Boolean(subtitle)),
          )
        }
        contentContainerStyle={{
          paddingHorizontal: 16,
          paddingTop: insets.top + peerInfo.avatarTop,
          paddingBottom: Math.max(insets.bottom, 16) + 24,
          gap: 24,
          maxWidth: 640,
          width: '100%',
          alignSelf: 'center',
        }}
      >
        <Stack.Screen
          options={{
            title: collapsed ? chat.title : '',
            headerTransparent: true,
            headerShadowVisible: false,
          }}
        />
        <ProfileHeader
          name={chat.title}
          photo={photo}
          tint={chat.counterpart?.tint_hex}
          agent={chat.counterpart?.kind === 'Agent'}
          subtitle={subtitle}
          actions={[
            {
              id: 'details-message',
              label: t('peerInfo.backToChat'),
              icon: { ios: 'bubble.left.fill', android: 'chat', web: 'chat' },
              onPress: () => router.dismissTo({ pathname: '/chat/[id]', params: { id } }),
            },
            {
              id: 'details-mute',
              label: t(muted ? 'peerInfo.unmute' : 'peerInfo.mute'),
              icon: muted
                ? { ios: 'bell.slash.fill', android: 'notifications_off', web: 'notifications_off' }
                : { ios: 'bell.fill', android: 'notifications', web: 'notifications' },
              disabled: muteDisabled,
              onPress: () => {
                if (mute.accountRequired) preference('muted', !chat.muted);
                else if (muted) {
                  mute.session.select(id, muteTicket);
                  void mute.session.mute(id, { kind: 'off' }, muteTicket);
                } else openMute();
              },
            },
            {
              id: 'details-search',
              label: t('peerInfo.search'),
              icon: { ios: 'magnifyingglass', android: 'search', web: 'search' },
              onPress: () => router.push({ pathname: '/search', params: { space: id } }),
            },
          ]}
        />
        <IntegrityWarning spaceId={id} />
        {error || mute.state.actionFailed || mute.state.readFailed ? (
          <Text accessibilityRole="alert" selectable style={{ color: p.destructive }}>
            {error ??
              t(
                mute.state.readFailed
                  ? 'notifications.mutedReadFailed'
                  : 'notifications.muteNotConfirmed',
              )}
          </Text>
        ) : null}
        <ProfileSection>
          <ProfileInfoRow label={t('peerInfo.privacy')} value={privacy} />
        </ProfileSection>
        <ProfileSection>
          <ProfileInfoRow
            testID="details-shared-media"
            label={t('peerInfo.shared')}
            onPress={() => router.push({ pathname: '/shared-media/[id]', params: { id } })}
          />
        </ProfileSection>
        <ProfileSection title={t('peerInfo.preferences')}>
          <Host
            matchContents={{ vertical: true }}
            style={{ width: '100%' }}
            colorScheme={scheme}
            seedColor={p.list.accent}
          >
            <Column
              spacing={0}
              modifiers={
                process.env.EXPO_OS === 'ios'
                  ? [frame({ maxWidth: Infinity, alignment: 'leading' })]
                  : undefined
              }
            >
              <Switch
                testID="details-preference-pinned"
                label={t('chat.pinChat')}
                value={chat.pinned}
                onValueChange={(value) => preference('pinned', value)}
                disabled={busy}
                modifiers={preferenceModifiers}
              />
              <Switch
                testID="details-preference-muted"
                label={t('chat.muteNotifications')}
                value={muted}
                onValueChange={(value) => {
                  if (mute.accountRequired) preference('muted', value);
                  else if (value) openMute();
                  else {
                    mute.session.select(id, muteTicket);
                    void mute.session.mute(id, { kind: 'off' }, muteTicket);
                  }
                }}
                disabled={muteDisabled}
                modifiers={preferenceModifiers}
              />
              <Switch
                testID="details-preference-archived"
                label={t('chat.archiveChat')}
                value={chat.archived}
                onValueChange={(value) => preference('archived', value)}
                disabled={busy}
                modifiers={preferenceModifiers}
              />
            </Column>
          </Host>
        </ProfileSection>
        <ProfileSection title={t('peerInfo.actions')}>
          <ProfileInfoRow
            label={t('conversationCards.manage')}
            onPress={() => router.push({ pathname: '/conversation-cards/[id]', params: { id } })}
          />
          <ProfileInfoRow
            label={t('chat.checkHistoryIntegrity')}
            onPress={() => router.push({ pathname: '/integrity/[id]', params: { id } })}
          />
          <ProfileInfoRow
            label={t('chat.createAPage')}
            onPress={() => router.push({ pathname: '/new-page', params: { space: id } })}
          />
        </ProfileSection>
        {chat.members.length ? (
          <ProfileSection title={t('chat.participantCount', { count: chat.members.length })}>
            {members.additionState?.canManage ? (
              <ProfileInfoRow
                testID="member-addition-open"
                label={t('community.add')}
                onPress={() =>
                  members.addition.open(members.addition.ticket(members.additionState))
                }
              />
            ) : null}
            {chat.members.map((person) => (
              <Pressable
                key={person.id}
                testID={`details-member-${person.id}`}
                accessibilityRole="button"
                accessibilityLabel={t('chat.viewPerson', { name: person.name })}
                accessibilityHint={
                  chat.kind !== 'Direct' && members.available && !person.is_me
                    ? t('community.actionsHint')
                    : undefined
                }
                accessibilityActions={
                  chat.kind !== 'Direct' && members.available && !person.is_me
                    ? [{ name: 'showMenu', label: t('community.actions') }]
                    : undefined
                }
                onAccessibilityAction={(event) => {
                  if (
                    event.nativeEvent.actionName === 'showMenu' &&
                    chat.kind !== 'Direct' &&
                    !person.is_me
                  )
                    members.session.open(person.id);
                }}
                onLongPress={
                  chat.kind !== 'Direct' && members.available && !person.is_me
                    ? () => members.session.open(person.id)
                    : undefined
                }
                onPress={() => router.push({ pathname: '/person/[id]', params: { id: person.id } })}
                style={({ pressed }) => ({
                  flexDirection: 'row',
                  alignItems: 'center',
                  gap: 13,
                  paddingHorizontal: 16,
                  paddingVertical: 9,
                  minHeight: 58,
                  backgroundColor: pressed ? p.list.highlighted : 'transparent',
                })}
              >
                <ProfileAvatar
                  person={person}
                  name={person.name}
                  color={person.tint_hex}
                  agent={person.kind === 'Agent'}
                  size={38}
                />
                <View style={{ flex: 1, gap: 3 }}>
                  <Text style={{ color: p.list.text, fontSize: 17 }}>
                    {person.is_me ? t('chat.personIsYou', { name: person.name }) : person.name}
                  </Text>
                  <Text style={{ color: p.list.secondary, fontSize: 13 }}>
                    {person.handle
                      ? `@${person.handle}`
                      : person.kind === 'Agent'
                        ? t('chat.agentLabel')
                        : t('chat.person')}
                  </Text>
                </View>
              </Pressable>
            ))}
          </ProfileSection>
        ) : null}
        {chat.kind !== 'Direct' ? (
          <ProfileSection>
            <ProfileInfoRow
              label={t('chat.inviteSomeone')}
              disabled={busy}
              onPress={() => {
                void perform(async () => {
                  const invite = await store.driver.request({ op: 'create_invite', space_id: id });
                  if (currentOwner()) await Share.share({ message: invite.link });
                });
              }}
            />
            <ProfileInfoRow
              destructive
              label={t('chat.leaveChat')}
              disabled={busy}
              onPress={() => {
                void confirm({
                  title: t('chat.leaveNamedChat', { title: chat.title }),
                  message: t('chat.youWillNeedANewInviteToReturn'),
                  confirmLabel: t('chat.leave'),
                  destructive: true,
                }).then((accepted) => {
                  if (accepted)
                    void perform(async () => {
                      await store.request({ op: 'leave_space', space_id: id });
                      if (currentOwner()) router.dismissTo('/');
                    });
                });
              }}
            />
          </ProfileSection>
        ) : null}
        <Text selectable style={{ color: p.list.secondary, fontSize: 13, textAlign: 'center' }}>
          {t('chat.historyEventTotal', { total: selected.event_count })}
        </Text>
      </ScrollView>
      <MuteDurationSheet
        key={`${mute.state.scope}:${mute.state.selected ?? ''}`}
        chat={mute.state.selected === id ? muteChat : null}
        clock={mute.state.clock_ms}
        busy={mute.state.phase !== 'ready' || mute.state.pending !== null}
        onDismiss={() => mute.session.select(null, muteTicket)}
        onChange={(choice) => {
          void mute.session.mute(id, choice, muteTicket);
        }}
        onUnmute={() => {
          void mute.session.mute(id, { kind: 'off' }, muteTicket);
        }}
      />

      {chat.kind !== 'Direct' && members.available ? (
        <>
          <MemberActionsSheet session={members.session} state={members.state} />
          <MemberAdditionSheet session={members.addition} state={members.additionState} />
        </>
      ) : null}
    </>
  );
}
