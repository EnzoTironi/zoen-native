import { router } from 'expo-router';
import { Text } from 'react-native';
import { useClient } from '../core/provider';
import { NativeButton, Surface } from '../components/primitives';
import { useTheme } from '../theme';

/** Cheap canonical known-error projection. Absence of a warning is not verification. */
export function IntegrityWarning({ spaceId }: { spaceId: string }) {
  const { snapshot } = useClient();
  const error = snapshot?.integrity_warnings[spaceId];
  const chat = snapshot?.conversations.find((chat) => chat.id === spaceId);
  const { colors } = useTheme();
  if (!error || !chat) return null;
  return (
    <Surface>
      <Text accessibilityRole="alert" style={{ color: colors.danger, fontWeight: '600' }}>
        Este histórico precisa de atenção
      </Text>
      <Text style={{ color: colors.secondary }}>
        {chat.title}. Confira outro dispositivo antes de confiar neste histórico.
      </Text>
      <NativeButton
        secondary
        label="Verificar histórico"
        onPress={() => router.push({ pathname: '/integrity/[id]', params: { id: spaceId } })}
      />
    </Surface>
  );
}

/** Optional account overview entry: only warnings are rendered. */
export function IntegrityWarnings() {
  const { snapshot } = useClient();
  return snapshot?.conversations
    .filter((chat) => snapshot.integrity_warnings[chat.id])
    .map((chat) => <IntegrityWarning key={chat.id} spaceId={chat.id} />);
}
