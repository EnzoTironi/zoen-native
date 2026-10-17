import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import { EmptyState, Loading, NativeButton, Surface } from '../../components/primitives';
import { useDialogs } from '../../components/dialogs';
import { useTheme } from '../../theme';
import { MiniappState } from '../../miniapps/state-view';
import { callMiniapp } from '../../miniapps/actions';

export default function Miniapp() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { store } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const read = useCoreQuery({ op: 'item', item_id: id });
  const specs = useCoreQuery({ op: 'app_specs' });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const item = read.data;
  const app = item?.app;
  if (!item)
    return read.error ? (
      <EmptyState title="Não foi possível abrir o miniapp" detail={read.error}>
        <NativeButton label="Tentar novamente" onPress={read.retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  if (!app)
    return (
      <EmptyState
        title="Este item não é um miniapp"
        detail="Volte à conversa para abrir o item correto."
      />
    );
  const spec = specs.data?.find((spec) => spec.id === app.app_id);
  const tools = spec?.tools.filter((tool) => tool.visibility.includes('app')) ?? [];
  function call(tool: string, args: Record<string, unknown>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    setMessage(null);
    void callMiniapp(store, dialogs, id, tool, JSON.stringify(args))
      .then((outcome) => {
        if (outcome) {
          setMessage(outcome.message || 'Alteração guardada.');
          read.retry();
        }
      })
      .catch((reason: unknown) =>
        setError(reason instanceof Error ? reason.message : 'Não foi possível concluir esta ação.'),
      )
      .finally(() => setBusy(false));
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 22,
        maxWidth: 850,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen
        options={{
          title: item.title,
          headerRight: () => (
            <NativeButton
              secondary
              label="Histórico"
              onPress={() => router.push({ pathname: '/item/[id]', params: { id, history: '1' } })}
            />
          ),
        }}
      />
      <Text style={{ color: colors.accent, fontWeight: '600', fontSize: 12 }}>
        {item.space_title} · VERSÃO {item.version}
      </Text>
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 32, fontWeight: '600' }}
      >
        {item.title}
      </Text>
      {error || read.error || specs.error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error ?? read.error ?? specs.error}
        </Text>
      ) : null}
      {message ? (
        <Text accessibilityLiveRegion="polite" selectable style={{ color: colors.accent }}>
          {message}
        </Text>
      ) : null}
      <MiniappState app={app} busy={busy} onCall={call} />
      <Surface>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}
        >
          Ações
        </Text>
        <Text style={{ color: colors.secondary, lineHeight: 22 }}>
          O núcleo verifica cada ação e pede sua confirmação quando necessário.
        </Text>
        <View style={{ gap: 12 }}>
          {tools
            .filter((tool) => !tool.read_only)
            .map((tool) => (
              <View key={tool.name} style={{ gap: 6, paddingVertical: 8 }}>
                <Text style={{ color: colors.secondary, fontSize: 14, lineHeight: 21 }}>
                  {tool.description}
                </Text>
                <NativeButton
                  secondary
                  label={tool.title}
                  disabled={busy}
                  onPress={() =>
                    router.push({ pathname: '/miniapp/[id]/tool', params: { id, tool: tool.name } })
                  }
                />
              </View>
            ))}
        </View>
        <NativeButton secondary label="Atualizar estado" disabled={busy} onPress={read.retry} />
      </Surface>
      <NativeButton
        secondary
        label="Permissões deste miniapp"
        onPress={() => router.push({ pathname: '/miniapp-permissions', params: { item: id } })}
      />
    </ScrollView>
  );
}
