import { t } from '../../i18n/index.ts';
import { useCallback, useRef, useState } from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useDialogs } from '../../components/dialogs';
import { useCoreQuery } from '../../core/query';
import { useProfilePhoto } from '../../profile/use-profile-photo';
import { useClient } from '../../core/provider';
import { EmptyState, Loading, NativeButton } from '../../components/primitives';
import { ProfileHeader } from '../../profile/profile-header';
import { ProfileInfoRow, ProfileSection } from '../../profile/profile-section';
import { ProfileAvatar } from '../../profile/profile-avatar';
import { peerInfo, profileTitleCollapseOffset } from '../../profile/telegram-profile-presentation';
import { useTheme } from '../../theme';

export default function Person() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { store, snapshot } = useClient();
  const { data: person, error, retry } = useCoreQuery({ op: 'profile', identity_id: id });
  const { presentation: p } = useTheme();
  const insets = useSafeAreaInsets();
  const { confirm } = useDialogs();
  const photo = useProfilePhoto(id, person?.photo_sha256, person?.photo_ready ?? false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
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
  const currentOwner = () => {
    const account = store.getSnapshot().snapshot?.account;
    return (
      active.current &&
      owner.current.id === id &&
      owner.current.epoch === store.sessionEpoch &&
      owner.current.identity === account?.identity_id &&
      owner.current.device === account?.device_id
    );
  };
  if (!person)
    return error ? (
      <EmptyState title={t('chat.profileUnavailable')} detail={error}>
        <NativeButton label={t('chat.tryAgain2')} onPress={retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  const selected = person;
  const agent = snapshot?.agents.find((item) => item.persona.id === id);
  const direct = snapshot?.conversations.find(
    (chat) => chat.kind === 'Direct' && chat.counterpart?.id === id,
  );
  const groups =
    snapshot?.conversations.filter(
      (chat) =>
        (chat.kind === 'Group' || chat.kind === 'Community') &&
        chat.members.some((member) => member.id === id),
    ) ?? [];
  async function act(action: () => Promise<unknown>) {
    if (pending.current || !currentOwner()) return;
    pending.current = true;
    setBusy(true);
    setFailure(null);
    try {
      await action();
    } catch (reason) {
      if (currentOwner())
        setFailure(reason instanceof Error ? reason.message : t('chat.thisCouldNotBeCompleted'));
    } finally {
      pending.current = false;
      if (currentOwner()) setBusy(false);
    }
  }
  return (
    <ScrollView
      testID="person-profile-screen"
      contentInsetAdjustmentBehavior="never"
      scrollEventThrottle={16}
      onScroll={(event) =>
        setCollapsed(event.nativeEvent.contentOffset.y > profileTitleCollapseOffset(true))
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
          title: collapsed ? (person.name ?? `@${person.handle}`) : '',
          headerTransparent: true,
          headerShadowVisible: false,
        }}
      />
      <ProfileHeader
        name={person.name ?? `@${person.handle}`}
        subtitle={`@${person.handle}`}
        photo={photo}
        tint={person.tint_hex}
        agent={!!agent}
        actions={
          person.is_me
            ? []
            : [
                {
                  id: 'person-message',
                  label: t('chat.chat'),
                  icon: { ios: 'bubble.left.fill', android: 'chat', web: 'chat' },
                  disabled: busy || person.blocked,
                  onPress: () => {
                    void act(async () => {
                      const space = await store.request({ op: 'start_direct', identity_id: id });
                      if (currentOwner())
                        router.push({ pathname: '/chat/[id]', params: { id: space } });
                    });
                  },
                },
              ]
        }
      />
      {failure || error ? (
        <Text selectable accessibilityRole="alert" style={{ color: p.destructive }}>
          {failure ?? error}
        </Text>
      ) : null}
      {person.bio ? (
        <ProfileSection>
          <ProfileInfoRow label={t('peerInfo.about')} value={person.bio} />
        </ProfileSection>
      ) : null}
      <ProfileSection>
        <ProfileInfoRow label={t('peerInfo.username')} value={`@${person.handle}`} />
      </ProfileSection>
      {person.is_me ? (
        <ProfileSection>
          <ProfileInfoRow
            label={t('chat.editMyProfile')}
            onPress={() => router.push('/edit-profile')}
          />
        </ProfileSection>
      ) : null}
      {direct ? (
        <ProfileSection>
          <ProfileInfoRow
            testID="person-shared-media"
            label={t('peerInfo.shared')}
            onPress={() =>
              router.push({ pathname: '/shared-media/[id]', params: { id: direct.id } })
            }
          />
        </ProfileSection>
      ) : null}
      {groups.length ? (
        <ProfileSection title={t('peerInfo.groups')}>
          {groups.map((chat) => (
            <Pressable
              key={chat.id}
              accessibilityRole="button"
              accessibilityLabel={chat.title}
              onPress={() => router.push({ pathname: '/space/[id]', params: { id: chat.id } })}
              style={({ pressed }) => ({
                minHeight: 58,
                paddingHorizontal: 16,
                paddingVertical: 9,
                flexDirection: 'row',
                gap: 13,
                alignItems: 'center',
                backgroundColor: pressed ? p.list.highlighted : 'transparent',
              })}
            >
              <ProfileAvatar name={chat.title} size={38} />
              <View style={{ flex: 1, gap: 3 }}>
                <Text numberOfLines={1} style={{ color: p.list.text, fontSize: 17 }}>
                  {chat.title}
                </Text>
                <Text style={{ color: p.list.secondary, fontSize: 13 }}>
                  {t('chat.participantCount', { count: chat.members.length })}
                </Text>
              </View>
            </Pressable>
          ))}
        </ProfileSection>
      ) : null}
      {agent ? (
        <ProfileSection>
          <ProfileInfoRow
            label={t('chat.permissionsAndBudget')}
            onPress={() => router.push({ pathname: '/agent/[id]', params: { id } })}
          />
        </ProfileSection>
      ) : null}
      {!person.is_me ? (
        <ProfileSection>
          <ProfileInfoRow
            testID="person-block"
            destructive={!person.blocked}
            disabled={busy}
            label={person.blocked ? t('chat.unblock') : t('chat.blockPerson')}
            onPress={() => {
              void confirm({
                title: selected.blocked ? t('chat.unblockThisPerson') : t('chat.blockThisPerson'),
                message: selected.blocked
                  ? t('chat.youWillBeAbleToChatWithThem')
                  : t('chat.youWillStopReceivingMessagesFromThisPerson'),
                confirmLabel: selected.blocked ? t('chat.unblock') : t('chat.block'),
                destructive: !selected.blocked,
              }).then((accepted) => {
                if (accepted && currentOwner())
                  void act(async () => {
                    await store.request({
                      op: selected.blocked ? 'unblock_person' : 'block_person',
                      identity_id: id,
                    });
                    if (currentOwner()) retry();
                  });
              });
            }}
          />
        </ProfileSection>
      ) : null}
    </ScrollView>
  );
}
