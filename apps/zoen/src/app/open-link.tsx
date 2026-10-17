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
      setFailure(reason instanceof Error ? reason.message : 'Não foi possível abrir este convite.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <View style={{ flex: 1, backgroundColor: colors.background }}>
      <Stack.Screen options={{ title: 'Abrir no Zoen' }} />
      {!link ? (
        <EmptyState
          title="Este link não é válido"
          detail="Peça um novo convite à pessoa que compartilhou este link."
        >
          <NativeButton label="Ir para conversas" onPress={() => router.replace('/')} />
        </EmptyState>
      ) : link.kind === 'invite' && !preview && !error ? (
        <Loading />
      ) : error && !preview ? (
        <EmptyState title="Não foi possível abrir o convite" detail={error}>
          <NativeButton label="Tentar novamente" onPress={retry} />
        </EmptyState>
      ) : (
        <EmptyState
          title={
            preview?.title ??
            (link.kind === 'agent' ? `@${link.target.replace(/^@/, '')}` : 'Convite do Zoen')
          }
          detail={
            preview
              ? `${preview.kind === 'agent' ? 'Convite de um agente' : `${preview.members} pessoas nesta conversa`}${preview.inviter ? ` · compartilhado por ${preview.inviter.name}` : ''}`
              : 'Abra uma conversa com este agente. As permissões continuam sob seu controle.'
          }
        >
          {preview?.agent ? (
            <Avatar name={preview.agent.name} color={preview.agent.tint_hex} agent size={64} />
          ) : null}
          <NativeButton
            label={
              busy
                ? 'Abrindo…'
                : preview?.kind === 'agent' || link.kind === 'agent'
                  ? 'Abrir conversa'
                  : 'Entrar na conversa'
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
