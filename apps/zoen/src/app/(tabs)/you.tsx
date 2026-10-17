import { Pressable, ScrollView, Text, View } from 'react-native';
import { useDialogs } from '../../components/dialogs';
import { router } from 'expo-router';
import { useProfilePhoto } from '../../profile/use-profile-photo';
import { useClient } from '../../core/provider';
import { useTheme } from '../../theme';
import { Avatar, NativeButton, Surface } from '../../components/primitives';

export default function You() {
  const { snapshot, store, mode } = useClient();
  const { colors, preference, setPreference, preferenceError } = useTheme();
  const { confirm } = useDialogs();
  const me = snapshot?.me;
  const photo = useProfilePhoto(me?.id, me?.photo_sha256, me?.photo_ready ?? false);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ padding: 24, gap: 24, backgroundColor: colors.background }}
    >
      <Text
        accessibilityRole="header"
        style={{ fontSize: 34, fontWeight: '700', color: colors.text }}
      >
        Você
      </Text>
      <Surface>
        <View style={{ flexDirection: 'row', gap: 18, alignItems: 'center' }}>
          <Avatar uri={photo} name={me?.name ?? 'Você'} color={me?.tint_hex} size={66} />
          <View style={{ flex: 1, gap: 5 }}>
            <Text style={{ fontSize: 23, fontWeight: '600', color: colors.text }}>
              {me?.name ?? 'Sua conta'}
            </Text>
            <Text style={{ fontSize: 15, color: colors.secondary }}>@{me?.handle}</Text>
          </View>
        </View>
        {me?.bio ? <Text style={{ color: colors.secondary, lineHeight: 23 }}>{me.bio}</Text> : null}
        <NativeButton
          secondary
          label="Editar meu perfil"
          onPress={() => router.push('/edit-profile')}
        />
      </Surface>
      <Surface>
        <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>Aparência</Text>
        <View style={{ flexDirection: 'row', gap: 10, flexWrap: 'wrap' }}>
          {(
            [
              { value: 'light', label: 'Claro' },
              { value: 'system', label: 'Sistema' },
              { value: 'dark', label: 'Escuro' },
            ] as const
          ).map((item) => (
            <NativeButton
              key={item.value}
              label={item.label}
              secondary={preference !== item.value}
              onPress={() => setPreference(item.value)}
            />
          ))}
        </View>
      </Surface>
      {preferenceError ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {preferenceError}
        </Text>
      ) : null}
      <Surface>
        <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>Seus agentes</Text>
        {snapshot?.agents.map((agent) => (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`Permissões de ${agent.persona.name}`}
            onPress={() =>
              router.push({ pathname: '/agent/[id]', params: { id: agent.persona.id } })
            }
            key={agent.persona.id}
            style={{ flexDirection: 'row', gap: 12, alignItems: 'center', paddingVertical: 8 }}
          >
            <Avatar name={agent.persona.name} color={agent.persona.tint_hex} agent size={40} />
            <View style={{ flex: 1 }}>
              <Text style={{ color: colors.text, fontSize: 16, fontWeight: '600' }}>
                {agent.persona.name}
              </Text>
              <Text style={{ color: colors.secondary, fontSize: 13 }}>{agent.runs_on}</Text>
            </View>
          </Pressable>
        ))}
      </Surface>
      <Surface>
        <Text style={{ color: colors.text, fontWeight: '600', fontSize: 18 }}>Conexão</Text>
        <NativeButton
          secondary
          label="Dispositivos vinculados"
          onPress={() => router.push('/devices')}
        />
        <NativeButton
          secondary
          label="Backup criptografado"
          onPress={() => router.push('/backup')}
        />
        <NativeButton
          secondary
          label="Notificações"
          onPress={() => router.push('/notifications')}
        />
        <Text style={{ color: colors.secondary }}>
          Estado:{' '}
          {snapshot?.connection.state === 'online'
            ? 'Conectado'
            : snapshot?.connection.state === 'connecting'
              ? 'Conectando'
              : 'Offline'}
        </Text>
        <Text style={{ color: colors.secondary }}>
          {snapshot?.connection.pending ?? '0'} mensagens aguardando sincronização
        </Text>
      </Surface>
      {mode === 'demo' ? (
        <NativeButton
          label="Usar uma conta de verdade"
          onPress={() => {
            void confirm({
              title: 'Sair da demonstração?',
              message: 'As conversas de demonstração ficam separadas da sua conta.',
            }).then((accepted) => {
              if (accepted) void store.open('account');
            });
          }}
        />
      ) : null}
    </ScrollView>
  );
}
