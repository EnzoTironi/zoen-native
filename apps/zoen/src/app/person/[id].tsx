import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import { useDialogs } from '../../components/dialogs';
import { useCoreQuery } from '../../core/query';
import { useClient } from '../../core/provider';
import { Avatar, EmptyState, Loading, NativeButton, Surface } from '../../components/primitives';
import { useTheme } from '../../theme';

export default function Person() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { store, snapshot } = useClient();
  const { data: person, error, retry } = useCoreQuery({ op: 'profile', identity_id: id });
  const { colors } = useTheme();
  const { confirm } = useDialogs();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  if (!person)
    return error ? (
      <EmptyState title="Perfil indisponível" detail={error}>
        <NativeButton label="Tentar novamente" onPress={retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  const selected = person;
  const agent = snapshot?.agents.find((item) => item.persona.id === id);
  async function act(action: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    try {
      await action();
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : 'Não foi possível concluir.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ padding: 26, gap: 24 }}
    >
      <Stack.Screen options={{ title: person.name ?? `@${person.handle}` }} />
      <View style={{ alignItems: 'center', gap: 14, paddingVertical: 24 }}>
        <Avatar
          name={person.name ?? person.handle}
          color={person.tint_hex}
          size={104}
          agent={!!agent}
        />
        <Text
          selectable
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 32, fontWeight: '600' }}
        >
          {person.name ?? `@${person.handle}`}
        </Text>
        <Text selectable style={{ color: colors.secondary, fontSize: 17 }}>
          @{person.handle}
        </Text>
      </View>
      {person.bio ? (
        <Surface>
          <Text selectable style={{ color: colors.text, fontSize: 17, lineHeight: 26 }}>
            {person.bio}
          </Text>
        </Surface>
      ) : null}
      {failure || error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
          {failure ?? error}
        </Text>
      ) : null}
      {person.is_me ? (
        <NativeButton label="Editar meu perfil" onPress={() => router.push('/edit-profile')} />
      ) : (
        <NativeButton
          label="Conversar"
          disabled={busy || person.blocked}
          onPress={() => {
            void act(async () => {
              const space = await store.request({ op: 'start_direct', identity_id: id });
              router.push({ pathname: '/chat/[id]', params: { id: space } });
            });
          }}
        />
      )}
      {agent ? (
        <NativeButton
          secondary
          label="Permissões e orçamento"
          onPress={() => router.push({ pathname: '/agent/[id]', params: { id } })}
        />
      ) : null}
      {!person.is_me ? (
        <NativeButton
          secondary
          label={person.blocked ? 'Desbloquear' : 'Bloquear pessoa'}
          disabled={busy}
          onPress={() => {
            void confirm({
              title: person.blocked ? 'Desbloquear esta pessoa?' : 'Bloquear esta pessoa?',
              message: person.blocked
                ? 'Você poderá conversar com ela novamente.'
                : 'Você deixará de receber mensagens desta pessoa.',
              confirmLabel: person.blocked ? 'Desbloquear' : 'Bloquear',
              destructive: !person.blocked,
            }).then((accepted) => {
              if (accepted)
                void act(() =>
                  store.request({
                    op: selected.blocked ? 'unblock_person' : 'block_person',
                    identity_id: id,
                  }),
                );
            });
          }}
        />
      ) : null}
      <Text
        selectable
        style={{ color: colors.tertiary, fontSize: 11, lineHeight: 17, textAlign: 'center' }}
      >
        {id}
      </Text>
    </ScrollView>
  );
}
