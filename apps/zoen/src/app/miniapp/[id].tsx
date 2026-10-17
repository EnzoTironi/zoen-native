import { t } from '../../i18n/index.ts';
import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import { EmptyState, Loading, NativeButton, Surface } from '../../components/primitives';
import { useDialogs } from '../../components/dialogs';
import { useTheme } from '../../theme';
import { MiniappState } from '../../miniapps/state-view';
import { MiniappView } from '../../miniapps/view-host';
import { callMiniapp } from '../../miniapps/actions';
import { MiniappSurfaceControls } from '../../native-surfaces/controls';

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
      <EmptyState title={t('miniapps.theMiniappCouldNotBeOpened')} detail={read.error}>
        <NativeButton label={t('miniapps.tryAgain')} onPress={read.retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  if (!app)
    return (
      <EmptyState
        title={t('miniapps.thisItemIsNotAMiniapp')}
        detail={t('miniapps.goBackToTheChatToOpenThe')}
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
          setMessage(outcome.message || t('miniapps.changeSaved'));
          read.retry();
        }
      })
      .catch((reason: unknown) =>
        setError(
          reason instanceof Error ? reason.message : t('miniapps.thisActionCouldNotBeCompleted'),
        ),
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
              label={t('miniapps.history')}
              onPress={() => router.push({ pathname: '/item/[id]', params: { id, history: '1' } })}
            />
          ),
        }}
      />
      <Text style={{ color: colors.accent, fontWeight: '600', fontSize: 12 }}>
        {t('miniapps.spaceVersion', { chat: item.space_title, version: item.version })}
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
      {spec?.has_view ? (
        <MiniappView item={item} spec={spec} />
      ) : (
        <MiniappState app={app} busy={busy} onCall={call} />
      )}
      <MiniappSurfaceControls item={item} />
      <Surface>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}
        >
          {t('miniapps.actions')}
        </Text>
        <Text style={{ color: colors.secondary, lineHeight: 22 }}>
          {t('miniapps.theCoreChecksEachActionAndAsksFor')}
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
        <NativeButton
          secondary
          label={t('miniapps.refreshState')}
          disabled={busy}
          onPress={read.retry}
        />
      </Surface>
      <NativeButton
        secondary
        label={t('miniapps.thisMiniappSPermissions')}
        onPress={() => router.push({ pathname: '/miniapp-permissions', params: { item: id } })}
      />
    </ScrollView>
  );
}
