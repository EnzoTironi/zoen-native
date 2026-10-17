import { IntegrityWarning } from '../../integrity/integrity-warning';
import { ProfileAvatar } from '../../profile/profile-avatar';
import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { Host, Column, Switch } from '@expo/ui';
import { Pressable, ScrollView, Share, Text, View } from 'react-native';
import { useDialogs } from '../../components/dialogs';
import { useClient } from '../../core/provider';
import { Avatar, EmptyState, Icon, NativeButton, Surface } from '../../components/primitives';
import { useTheme } from '../../theme';

export default function Space() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { snapshot, store } = useClient();
  const { colors, scheme } = useTheme();
  const { confirm } = useDialogs();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const chat = snapshot?.conversations.find((item) => item.id === id);
  if (!chat)
    return (
      <EmptyState
        title="Conversa indisponível"
        detail="Esta conversa não está mais neste aparelho."
      />
    );
  const selected = chat;
  async function perform(action: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : 'Não foi possível concluir.');
    } finally {
      setBusy(false);
    }
  }
  function preference(key: 'pinned' | 'muted' | 'archived', value: boolean) {
    void perform(() =>
      store.request({
        op: 'set_chat_prefs',
        space_id: id,
        pinned: selected.pinned,
        muted: selected.muted,
        archived: selected.archived,
        [key]: value,
      }),
    );
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ padding: 24, gap: 22 }}
    >
      <Stack.Screen options={{ title: 'Detalhes' }} />
      <View style={{ alignItems: 'center', gap: 16, paddingVertical: 16 }}>
        <Avatar
          name={chat.title}
          size={90}
          color={chat.counterpart?.tint_hex}
          agent={chat.counterpart?.kind === 'Agent'}
        />
        <Text
          selectable
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 30, fontWeight: '600', textAlign: 'center' }}
        >
          {chat.title}
        </Text>
        <View style={{ flexDirection: 'row', gap: 7, alignItems: 'center' }}>
          <Icon
            name={{
              ios:
                chat.privacy === 'EndToEnd'
                  ? 'lock.shield'
                  : chat.privacy === 'Closed'
                    ? 'shield'
                    : 'globe',
              android:
                chat.privacy === 'EndToEnd'
                  ? 'encrypted'
                  : chat.privacy === 'Closed'
                    ? 'shield'
                    : 'public',
              web:
                chat.privacy === 'EndToEnd'
                  ? 'encrypted'
                  : chat.privacy === 'Closed'
                    ? 'shield'
                    : 'public',
            }}
            size={16}
            color={colors.secondary}
          />
          <Text style={{ color: colors.secondary, fontSize: 13 }}>
            {chat.privacy === 'EndToEnd'
              ? 'Criptografia de ponta a ponta'
              : chat.privacy === 'Closed'
                ? 'Conversa fechada · legível pelo servidor'
                : 'Conversa pública'}
          </Text>
        </View>
      </View>
      <IntegrityWarning spaceId={id} />
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <Surface>
        <Host matchContents colorScheme={scheme} seedColor={colors.accent}>
          <Column spacing={18}>
            <Switch
              label="Fixar conversa"
              value={chat.pinned}
              onValueChange={(value) => preference('pinned', value)}
              disabled={busy}
            />
            <Switch
              label="Silenciar notificações"
              value={chat.muted}
              onValueChange={(value) => preference('muted', value)}
              disabled={busy}
            />
            <Switch
              label="Arquivar conversa"
              value={chat.archived}
              onValueChange={(value) => preference('archived', value)}
              disabled={busy}
            />
          </Column>
        </Host>
      </Surface>
      <Surface>
        <NativeButton
          secondary
          label="Buscar nesta conversa"
          onPress={() => router.push({ pathname: '/search', params: { space: id } })}
        />
        <NativeButton
          secondary
          label="Verificar integridade do histórico"
          onPress={() => router.push({ pathname: '/integrity/[id]', params: { id } })}
        />
        <NativeButton
          secondary
          label="Criar uma página"
          onPress={() => router.push({ pathname: '/new-page', params: { space: id } })}
        />
      </Surface>
      {chat.members.length ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
            {chat.members.length} participantes
          </Text>
          {chat.members.map((person) => (
            <Pressable
              key={person.id}
              accessibilityRole="button"
              accessibilityLabel={`Ver ${person.name}`}
              onPress={() => router.push({ pathname: '/person/[id]', params: { id: person.id } })}
              style={{ flexDirection: 'row', alignItems: 'center', gap: 13, paddingVertical: 9 }}
            >
              <ProfileAvatar
                person={person}
                name={person.name}
                color={person.tint_hex}
                agent={person.kind === 'Agent'}
                size={38}
              />
              <View style={{ flex: 1, gap: 4 }}>
                <Text style={{ color: colors.text, fontSize: 16, fontWeight: '500' }}>
                  {person.name}
                  {person.is_me ? ' (você)' : ''}
                </Text>
                <Text style={{ color: colors.secondary, fontSize: 12 }}>
                  {person.handle
                    ? `@${person.handle}`
                    : person.kind === 'Agent'
                      ? 'Agente'
                      : 'Pessoa'}
                </Text>
              </View>
            </Pressable>
          ))}
        </Surface>
      ) : null}
      {chat.kind !== 'Direct' ? (
        <NativeButton
          label="Convidar alguém"
          disabled={busy}
          onPress={() => {
            void perform(async () => {
              const invite = await store.driver.request({ op: 'create_invite', space_id: id });
              await Share.share({ message: invite.link });
            });
          }}
        />
      ) : null}
      {chat.kind !== 'Direct' ? (
        <NativeButton
          secondary
          label="Sair da conversa"
          disabled={busy}
          onPress={() => {
            void confirm({
              title: `Sair de ${chat.title}?`,
              message: 'Você precisará de um novo convite para voltar.',
              confirmLabel: 'Sair',
              destructive: true,
            }).then((accepted) => {
              if (accepted)
                void perform(async () => {
                  await store.request({ op: 'leave_space', space_id: id });
                  router.dismissTo('/');
                });
            });
          }}
        />
      ) : null}
      <Text selectable style={{ color: colors.tertiary, fontSize: 11, textAlign: 'center' }}>
        {chat.event_count} eventos no histórico
      </Text>
    </ScrollView>
  );
}
