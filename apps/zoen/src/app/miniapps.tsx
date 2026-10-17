import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import type { AppSpecDto } from '../core/generated/AppSpecDto';
import { EmptyState, Loading, NativeButton, Surface } from '../components/primitives';
import { useTheme } from '../theme';
import { SchemaForm } from '../miniapps/schema-form';
import { exactJson, formValue, parseSchema } from '../miniapps/schema';
import { useDirtyEditor } from '../documents/use-dirty-editor';
import { useDialogs } from '../components/dialogs';

function Install({
  spec,
  initialSpace,
  onBack,
}: {
  spec: AppSpecDto;
  initialSpace: string;
  onBack: () => void;
}) {
  const { store, snapshot } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const [space, setSpace] = useState(initialSpace);
  const [draft, setDraft] = useState<unknown>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const finish = useDirtyEditor(JSON.stringify(draft) !== '{}', busy);
  const creator = spec.tools.find(
    (tool) => tool.visibility.includes('model') && !tool.visibility.includes('app'),
  );
  if (!creator)
    return (
      <EmptyState
        title="Este miniapp ainda não pode ser instalado"
        detail="O catálogo não forneceu um formulário de criação."
      />
    );
  let schema;
  try {
    schema = parseSchema(creator.input_schema_json);
  } catch (reason) {
    return (
      <EmptyState
        title="Formulário indisponível"
        detail={
          reason instanceof Error ? reason.message : 'Não foi possível abrir este formulário.'
        }
      />
    );
  }
  return (
    <>
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 28, fontWeight: '600' }}
      >
        {spec.name}
      </Text>
      <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 24 }}>
        {spec.description}
      </Text>
      <Text style={{ color: colors.secondary, fontWeight: '600' }}>Adicionar na conversa</Text>
      <View style={{ gap: 8 }}>
        {snapshot?.conversations
          .filter((chat) => chat.members.some((person) => person.is_me))
          .map((chat) => (
            <Pressable
              key={chat.id}
              disabled={busy}
              accessibilityRole="radio"
              aria-checked={space === chat.id}
              onPress={() => setSpace(chat.id)}
              style={{
                minHeight: 48,
                padding: 15,
                borderRadius: 16,
                backgroundColor: space === chat.id ? colors.muted : colors.surface,
              }}
            >
              <Text style={{ color: colors.text }}>
                {space === chat.id ? '●' : '○'} {chat.title}
              </Text>
            </Pressable>
          ))}
      </View>
      <SchemaForm schema={schema} value={draft} onChange={setDraft} disabled={busy} />
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <NativeButton
        label={busy ? 'Adicionando…' : 'Adicionar miniapp'}
        disabled={busy || !space}
        onPress={() => {
          setBusy(true);
          setError(null);
          void (async () => {
            const args_json = exactJson(formValue(schema, draft)!);
            const item = await store.request({
              op: 'install_app',
              space_id: space,
              app_id: spec.id,
              args_json,
            });
            finish();
            router.replace({ pathname: '/miniapp/[id]', params: { id: item.id } });
          })()
            .catch((reason: unknown) =>
              setError(
                reason instanceof Error
                  ? reason.message
                  : 'Não foi possível instalar este miniapp.',
              ),
            )
            .finally(() => setBusy(false));
        }}
      />
      <NativeButton
        secondary
        label="Voltar ao catálogo"
        disabled={busy}
        onPress={() => {
          void (async () => {
            if (
              JSON.stringify(draft) !== '{}' &&
              !(await dialogs.confirm({
                title: 'Descartar este formulário?',
                message:
                  'O miniapp ainda não foi criado. Seus dados neste formulário serão descartados.',
                confirmLabel: 'Descartar',
                destructive: true,
              }))
            )
              return;
            finish();
            onBack();
          })();
        }}
      />
    </>
  );
}
export default function Miniapps() {
  const { space: initialSpace } = useLocalSearchParams<{ space?: string }>();
  const { colors } = useTheme();
  const read = useCoreQuery({ op: 'app_specs' });
  const [selected, setSelected] = useState<AppSpecDto | null>(null);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 20,
        maxWidth: 850,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: 'Miniapps' }} />
      {selected ? (
        <Install
          key={selected.id}
          spec={selected}
          initialSpace={initialSpace ?? ''}
          onBack={() => setSelected(null)}
        />
      ) : (
        <>
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 32, fontWeight: '600' }}
          >
            Pequenas coisas, juntas
          </Text>
          <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 24 }}>
            Listas, votações, receitas e agendas compartilhadas nas suas conversas.
          </Text>
          <NativeButton
            secondary
            label="Permissões dos miniapps"
            onPress={() => router.push('/miniapp-permissions')}
          />
          {!read.data ? (
            read.error ? (
              <EmptyState title="Não foi possível abrir o catálogo" detail={read.error}>
                <NativeButton label="Tentar novamente" onPress={read.retry} />
              </EmptyState>
            ) : (
              <Loading />
            )
          ) : (
            read.data.map((spec) => (
              <Surface key={spec.id}>
                <Text style={{ color: colors.text, fontSize: 23, fontWeight: '600' }}>
                  {spec.name}
                </Text>
                <Text style={{ color: colors.secondary, fontSize: 15, lineHeight: 23 }}>
                  {spec.description}
                </Text>
                <NativeButton
                  secondary
                  label={`Adicionar ${spec.name}`}
                  onPress={() => setSelected(spec)}
                />
              </Surface>
            ))
          )}
        </>
      )}
    </ScrollView>
  );
}
