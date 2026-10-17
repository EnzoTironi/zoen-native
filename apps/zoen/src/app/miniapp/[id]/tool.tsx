import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text } from 'react-native';
import type { AppToolDto } from '../../../core/generated/AppToolDto';
import { useClient } from '../../../core/provider';
import { useCoreQuery } from '../../../core/query';
import { EmptyState, Loading, NativeButton } from '../../../components/primitives';
import { useDialogs } from '../../../components/dialogs';
import { useTheme } from '../../../theme';
import { SchemaForm } from '../../../miniapps/schema-form';
import { exactJson, formValue, parseSchema } from '../../../miniapps/schema';
import { callMiniapp } from '../../../miniapps/actions';
import { useDirtyEditor } from '../../../documents/use-dirty-editor';

function Form({ item, tool }: { item: string; tool: AppToolDto }) {
  const { store } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const [draft, setDraft] = useState<unknown>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const finish = useDirtyEditor(JSON.stringify(draft) !== '{}', busy);
  let schema;
  try {
    schema = parseSchema(tool.input_schema_json);
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
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 22,
        maxWidth: 760,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: tool.title }} />
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 28, fontWeight: '600' }}
      >
        {tool.title}
      </Text>
      <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 24 }}>
        {tool.description}
      </Text>
      <SchemaForm schema={schema} value={draft} onChange={setDraft} disabled={busy} />
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <NativeButton
        label={busy ? 'Aguarde…' : tool.title}
        disabled={busy}
        onPress={() => {
          setBusy(true);
          setError(null);
          void (async () => {
            const args = exactJson(formValue(schema, draft)!);
            const outcome = await callMiniapp(store, dialogs, item, tool.name, args);
            if (outcome) {
              finish();
              router.back();
            }
          })()
            .catch((reason: unknown) =>
              setError(
                reason instanceof Error ? reason.message : 'Não foi possível concluir esta ação.',
              ),
            )
            .finally(() => setBusy(false));
        }}
      />
    </ScrollView>
  );
}
export default function MiniappTool() {
  const { id, tool: name } = useLocalSearchParams<{ id: string; tool: string }>();
  const item = useCoreQuery({ op: 'item', item_id: id });
  const specs = useCoreQuery({ op: 'app_specs' });
  if (!item.data || !specs.data) {
    const error = item.error ?? specs.error;
    return error ? (
      <EmptyState title="Não foi possível abrir a ação" detail={error}>
        <NativeButton
          label="Tentar novamente"
          onPress={() => {
            item.retry();
            specs.retry();
          }}
        />
      </EmptyState>
    ) : (
      <Loading />
    );
  }
  const tool = specs.data
    .find((spec) => spec.id === item.data?.app?.app_id)
    ?.tools.find((tool) => tool.name === name && tool.visibility.includes('app'));
  if (!tool)
    return (
      <EmptyState
        title="Ação indisponível"
        detail="Este miniapp não permite abrir esta ação pela interface."
      />
    );
  return <Form key={`${id}:${name}`} item={id} tool={tool} />;
}
