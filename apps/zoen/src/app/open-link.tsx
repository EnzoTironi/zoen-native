import { t } from '../i18n/index.ts';
import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { Text, View } from 'react-native';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { parseZoenLink } from '../notifications/links';
import { useTheme } from '../theme';
import { Avatar, EmptyState, Loading, NativeButton } from '../components/primitives';

export default function OpenLink() {
  const { kind, target } = useLocalSearchParams<{ kind?: string; target?: string }>();
  const link =
    target && ['invite', 'agent'].includes(kind ?? '')
      ? parseZoenLink(
          `zoen://${kind === 'invite' ? 'join' : 'agent'}/${encodeURIComponent(target)}`,
        )
      : null;
  const { store } = useClient();
  const { colors } = useTheme();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const {
    data: preview,
    error,
    retry,
  } = useCoreQuery(link?.kind === 'invite' ? { op: 'preview_invite', code: link.code } : null);
  async function open() {
    if (!link || busy) return;
    setBusy(true);
    setFailure(null);
    try {
      const space =
        link.kind === 'invite'
          ? await store.request({ op: 'join_invite', code: link.code })
          : link.kind === 'agent'
            ? await store.request({ op: 'open_agent_link', target: link.target })
            : null;
      if (space) router.replace({ pathname: '/chat/[id]', params: { id: space } });
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : t('chat.thisInviteCouldNotBeOpened'));
    } finally {
      setBusy(false);
    }
  }
  return (
    <View style={{ flex: 1, backgroundColor: colors.background }}>
      <Stack.Screen options={{ title: t('chat.openInZoen') }} />
      {!link ? (
        <EmptyState
          title={t('chat.thisLinkIsInvalid')}
          detail={t('chat.askThePersonWhoSharedThisLinkFor')}
        >
          <NativeButton label={t('chat.goToChats')} onPress={() => router.replace('/')} />
        </EmptyState>
      ) : link.kind === 'invite' && !preview && !error ? (
        <Loading />
      ) : error && !preview ? (
        <EmptyState title={t('chat.theInviteCouldNotBeOpened')} detail={error}>
          <NativeButton label={t('chat.tryAgain2')} onPress={retry} />
        </EmptyState>
      ) : (
        <EmptyState
          title={
            preview?.title ??
            (link.kind === 'agent' ? `@${link.target.replace(/^@/, '')}` : t('chat.zoenInvite'))
          }
          detail={
            preview
              ? preview.inviter
                ? t('chat.inviteSharedBy', {
                    summary:
                      preview.kind === 'agent'
                        ? t('chat.agentInvite')
                        : t('chat.inviteMemberCount', { count: preview.members }),
                    name: preview.inviter.name,
                  })
                : preview.kind === 'agent'
                  ? t('chat.agentInvite')
                  : t('chat.inviteMemberCount', { count: preview.members })
              : t('chat.openAChatWithThisAgentYouStay')
          }
        >
          {preview?.agent ? (
            <Avatar name={preview.agent.name} color={preview.agent.tint_hex} agent size={64} />
          ) : null}
          <NativeButton
            label={
              busy
                ? t('chat.opening')
                : preview?.kind === 'agent' || link.kind === 'agent'
                  ? t('chat.openChat')
                  : t('chat.joinChat')
            }
            disabled={busy}
            onPress={() => {
              void open();
            }}
          />
          {failure ? (
            <Text
              accessibilityRole="alert"
              selectable
              style={{ color: colors.danger, textAlign: 'center', lineHeight: 22 }}
            >
              {failure}
            </Text>
          ) : null}
        </EmptyState>
      )}
    </View>
  );
}
