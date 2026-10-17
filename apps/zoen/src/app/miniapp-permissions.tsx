import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text } from 'react-native';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { EmptyState, Loading, NativeButton, Surface } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useTheme } from '../theme';

const labels: Record<string, string> = {
  'photos.pick': 'Escolher fotos',
  location: 'Localização',
  'calendar.freebusy': 'Disponibilidade na agenda',
  'calendar.write': 'Criar eventos na agenda',
  camera: 'Câmera',
};
export default function MiniappPermissions() {
  const { item } = useLocalSearchParams<{ item?: string }>();
  const { store } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const read = useCoreQuery({ op: 'app_device_grants' });
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const grants = read.data?.filter((grant) => !item || grant.item_id === item);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 20,
        maxWidth: 800,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: 'Permissões dos miniapps' }} />
      <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 24 }}>
        Cada permissão pertence a um miniapp de uma conversa. Revogue uma e ele terá que pedir
        novamente.
      </Text>
      {error || read.error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error ?? read.error}
        </Text>
      ) : null}
      {!grants ? (
        read.error ? (
          <NativeButton label="Tentar novamente" onPress={read.retry} />
        ) : (
          <Loading />
        )
      ) : !grants.length ? (
        <EmptyState
          title="Nenhuma permissão ativa"
          detail="Quando você permitir acesso a um recurso, o miniapp e o motivo aparecerão aqui."
        />
      ) : (
        grants.map((grant) => (
          <Surface key={grant.grant_id}>
            <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>
              {grant.item_title}
            </Text>
            <Text style={{ color: colors.accent }}>
              {labels[grant.capability] ??
                (grant.capability.startsWith('net:')
                  ? `Acesso a ${grant.capability.slice(4)}`
                  : grant.capability)}
            </Text>
            <Text selectable style={{ color: colors.secondary, lineHeight: 22 }}>
              {grant.purpose}
            </Text>
            <Text style={{ color: colors.secondary, fontSize: 13 }}>
              {grant.space_title} · {grant.always ? 'Até você revogar' : 'Permissão temporária'}
            </Text>
            <NativeButton
              secondary
              label="Abrir miniapp"
              disabled={busy !== null}
              onPress={() =>
                router.push({ pathname: '/miniapp/[id]', params: { id: grant.item_id } })
              }
            />
            <NativeButton
              secondary
              label={busy === grant.grant_id ? 'Revogando…' : 'Revogar permissão'}
              disabled={busy !== null}
              onPress={() => {
                void dialogs
                  .confirm({
                    title: 'Revogar esta permissão?',
                    message: `${grant.item_title}\n${labels[grant.capability] ?? grant.capability}\n\nO miniapp precisará pedir novamente.`,
                    confirmLabel: 'Revogar',
                    destructive: true,
                  })
                  .then((accepted) => {
                    if (!accepted) return;
                    setBusy(grant.grant_id);
                    setError(null);
                    void store
                      .request({ op: 'revoke_app_device', grant_id: grant.grant_id })
                      .then(read.retry)
                      .catch((reason: unknown) =>
                        setError(
                          reason instanceof Error ? reason.message : 'Não foi possível revogar.',
                        ),
                      )
                      .finally(() => setBusy(null));
                  });
              }}
            />
          </Surface>
        ))
      )}
    </ScrollView>
  );
}
