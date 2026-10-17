import { useState } from 'react';
import { Stack, router } from 'expo-router';
import { ScrollView, Text } from 'react-native';
import { useClient } from '../core/provider';
import { Field, NativeButton } from '../components/primitives';
import { useTheme } from '../theme';

export default function EditProfile() {
  const { snapshot, store } = useClient();
  const { colors } = useTheme();
  const [name, setName] = useState(snapshot?.me?.name ?? '');
  const [bio, setBio] = useState(snapshot?.me?.bio ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{ padding: 26, gap: 24 }}
    >
      <Stack.Screen options={{ title: 'Seu perfil' }} />
      <Field label="Nome" value={name} onChangeText={setName} autoComplete="name" maxLength={100} />
      <Field
        label="Sobre você"
        value={bio}
        onChangeText={setBio}
        multiline
        maxLength={1000}
        style={{ minHeight: 120, textAlignVertical: 'top' }}
      />
      <Text style={{ color: colors.secondary, lineHeight: 22 }}>
        Seu perfil é compartilhado de forma criptografada com seus contatos.
      </Text>
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <NativeButton
        label={busy ? 'Salvando…' : 'Salvar'}
        disabled={busy || !name.trim()}
        onPress={() => {
          setBusy(true);
          setError(null);
          void store
            .request({ op: 'update_profile', name: name.trim(), bio })
            .then(() => router.back())
            .catch((failure: unknown) =>
              setError(
                failure instanceof Error ? failure.message : 'Não foi possível salvar seu perfil.',
              ),
            )
            .finally(() => setBusy(false));
        }}
      />
    </ScrollView>
  );
}
