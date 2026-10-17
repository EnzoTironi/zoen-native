import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import { Field, NativeButton } from '../components/primitives';
import { useTheme } from '../theme';
import { pickMarkdown } from '../documents/markdown-files';
import { useDirtyEditor } from '../documents/use-dirty-editor';

export default function NewPage() {
  const { space: initial } = useLocalSearchParams<{ space?: string }>();
  const { snapshot, store } = useClient();
  const { colors } = useTheme();
  const [space, setSpace] = useState(initial ?? '');
  const [title, setTitle] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [imported, setImported] = useState<{ path: string; markdown: string } | null>(null);
  const finish = useDirtyEditor(!!title.trim() || imported !== null, busy);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{ padding: 26, gap: 24 }}
    >
      <Stack.Screen options={{ title: imported ? 'Importar página' : 'Nova página' }} />
      {imported ? (
        <View style={{ gap: 12 }}>
          <Text selectable style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}>
            {imported.path}
          </Text>
          <Text
            selectable
            numberOfLines={10}
            style={{
              color: colors.secondary,
              fontSize: 15,
              lineHeight: 23,
              fontFamily: process.env.EXPO_OS === 'ios' ? 'Menlo' : 'monospace',
            }}
          >
            {imported.markdown || 'Arquivo vazio'}
          </Text>
          <NativeButton
            secondary
            label="Criar uma página vazia"
            disabled={busy}
            onPress={() => setImported(null)}
          />
        </View>
      ) : (
        <Field
          label="Título"
          value={title}
          onChangeText={setTitle}
          autoFocus
          placeholder="Uma ideia para começar"
        />
      )}
      <NativeButton
        secondary
        label={imported ? 'Escolher outro Markdown' : 'Importar arquivo Markdown'}
        disabled={busy}
        onPress={() => {
          setBusy(true);
          setError(null);
          void pickMarkdown()
            .then((file) => {
              if (file) setImported(file);
            })
            .catch((reason: unknown) =>
              setError(
                reason instanceof Error ? reason.message : 'Não foi possível abrir este arquivo.',
              ),
            )
            .finally(() => setBusy(false));
        }}
      />
      <View style={{ gap: 10 }}>
        <Text style={{ color: colors.secondary, fontSize: 14, fontWeight: '600' }}>
          Compartilhar na conversa
        </Text>
        {snapshot?.conversations.map((chat) => (
          <Pressable
            key={chat.id}
            accessibilityRole="radio"
            accessibilityState={{ checked: space === chat.id }}
            onPress={() => setSpace(chat.id)}
            style={{
              padding: 18,
              borderRadius: 18,
              backgroundColor: space === chat.id ? colors.muted : colors.surface,
              flexDirection: 'row',
              gap: 14,
            }}
          >
            <Text style={{ flex: 1, color: colors.text, fontSize: 16 }}>{chat.title}</Text>
            <Text style={{ color: colors.accent }}>{space === chat.id ? '✓' : ''}</Text>
          </Pressable>
        ))}
      </View>
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <NativeButton
        disabled={busy || !space || (!imported && !title.trim())}
        label={busy ? 'Guardando…' : imported ? 'Importar e compartilhar página' : 'Criar página'}
        onPress={() => {
          setBusy(true);
          setError(null);
          void store
            .request(
              imported
                ? {
                    op: 'page_import_markdown',
                    space_id: space,
                    path: imported.path,
                    markdown: imported.markdown,
                  }
                : { op: 'page_create', space_id: space, title: title.trim() },
            )
            .then((item) => {
              finish();
              router.replace({ pathname: '/page/[id]', params: { id: item.id } });
            })
            .catch((failure: unknown) =>
              setError(
                failure instanceof Error ? failure.message : 'Não foi possível criar a página.',
              ),
            )
            .finally(() => setBusy(false));
        }}
      />
    </ScrollView>
  );
}
