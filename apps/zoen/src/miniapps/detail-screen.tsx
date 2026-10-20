import { t } from '../i18n/index.ts';
import { useCallback, useRef, useState } from 'react';
import { Stack, router, useLocalSearchParams, usePreventRemove } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { EmptyState, Loading, NativeButton, Surface } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useTheme } from '../theme';
import { MiniappState } from './state-view';
import { MiniappView } from './view-host';
import { callMiniapp } from './actions';
import { MiniappSurfaceControls } from '../native-surfaces/controls';
import { useHome } from '../home/provider';
import { HomeCardMenu } from '../home/menus';
import { useConversationCards } from './conversation-provider';
import type { MiniappDocumentController } from './document-controller';

export function MiniappScreen({ manage = false }: { manage?: boolean }) {
  const insets = useSafeAreaInsets();
  const { id } = useLocalSearchParams<{ id: string }>();
  const { store } = useClient();
  const { colors } = useTheme();
  const home = useHome();
  const dialogs = useDialogs();
  const read = useCoreQuery({ op: 'item', item_id: id });
  const specs = useCoreQuery({ op: 'app_specs' });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [document, setDocument] = useState<MiniappDocumentController | null>(null);
  const [leaving, setLeaving] = useState(false);
  const navigating = useRef(false);
  const registerDocument = useCallback((controller: MiniappDocumentController | null) => {
    setDocument(controller);
    if (controller) setLeaving(false);
  }, []);
  const disablePrevention = usePreventRemove(!!document && !leaving, ({ repeat }) => {
    if (!document || navigating.current) return;
    navigating.current = true;
    void document
      .flush({ retire: true })
      .then(() => {
        setLeaving(true);
        repeat();
      })
      .catch(() => setError(t('drawing.closeFailed')))
      .finally(() => {
        navigating.current = false;
      });
  });
  const openCopy = useCallback(
    (itemId: string) => {
      setLeaving(true);
      disablePrevention();
      router.replace({ pathname: '/miniapp/[id]', params: { id: itemId } });
    },
    [disablePrevention],
  );
  const discardDocument = useCallback(() => {
    if (navigating.current) return;
    navigating.current = true;
    void dialogs
      .confirm({
        title: t('drawing.discard'),
        message: t('drawing.discardDetails'),
        confirmLabel: t('miniapps.discard'),
        destructive: true,
      })
      .then((accepted) => {
        if (!accepted) return;
        setLeaving(true);
        disablePrevention();
        router.back();
      })
      .finally(() => {
        navigating.current = false;
      });
  }, [dialogs, disablePrevention]);
  const item = read.data;
  const cards = useConversationCards(item?.space_id ?? '');
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
  const tools =
    spec?.tools.filter(
      (tool) =>
        tool.visibility.includes('app') &&
        !(spec.id === 'excalidraw' && tool.name.startsWith('app_document_')),
    ) ?? [];
  function openDetails() {
    if (navigating.current) return;
    navigating.current = true;
    void (async () => {
      await document?.flush({ retire: true });
      setError(null);
      router.push({ pathname: '/miniapp/[id]/manage', params: { id } });
    })()
      .catch(() => setError(t('drawing.closeFailed')))
      .finally(() => {
        navigating.current = false;
      });
  }
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
  const fullscreen = !!spec?.has_view && !manage;
  const navigation = (
    <Stack.Screen
      options={{
        title: manage ? t('common.details') : item.title,
        // Interactive app views own their drags, including the back-swipe edge.
        // Close, system Back and native sheet dismissal share the document-close guard.
        gestureEnabled: !fullscreen,
        headerRight: () =>
          fullscreen ? (
            <NativeButton
              secondary
              testID="miniapp-open-details"
              label={t('common.details')}
              onPress={openDetails}
            />
          ) : process.env.EXPO_OS === 'ios' ? (
            <HomeCardMenu
              testID="miniapp-detail-options"
              name={item.title}
              pinned={home.isPinned(item.id)}
              onToggle={
                !home.loading && home.items.some((candidate) => candidate.id === item.id)
                  ? () => {
                      void (home.isPinned(item.id) ? home.unpin(item.id) : home.pin(item.id)).catch(
                        () => {},
                      );
                    }
                  : undefined
              }
              conversationPinned={cards.isPinned(item.id)}
              onToggleConversation={
                !cards.loading && cards.items.some((candidate) => candidate.id === item.id)
                  ? () => {
                      void cards.setPinned(item.id, !cards.isPinned(item.id)).catch(() => {});
                    }
                  : undefined
              }
              onOrganizeConversation={
                cards.available
                  ? () =>
                      router.push({
                        pathname: '/conversation-cards/[id]',
                        params: { id: item.space_id },
                      })
                  : undefined
              }
              onHistory={() =>
                router.push({ pathname: '/item/[id]', params: { id, history: '1' } })
              }
            />
          ) : (
            <NativeButton
              secondary
              label={t('miniapps.history')}
              onPress={() => router.push({ pathname: '/item/[id]', params: { id, history: '1' } })}
            />
          ),
      }}
    />
  );
  if (fullscreen && spec) {
    return (
      <View
        testID="miniapp-fullscreen"
        style={{
          flex: 1,
          paddingBottom: insets.bottom,
          paddingLeft: insets.left,
          paddingRight: insets.right,
        }}
      >
        {navigation}
        {error || read.error || specs.error ? (
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger, padding: 16 }}>
            {error ?? read.error ?? specs.error}
          </Text>
        ) : null}
        <MiniappView
          item={item}
          spec={spec}
          fill
          onDocumentController={registerDocument}
          onDocumentCopied={openCopy}
          onDocumentDiscard={discardDocument}
        />
      </View>
    );
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        paddingTop: 24,
        paddingBottom: 48,
        gap: 22,
        maxWidth: 850,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      {navigation}
      <Text style={{ color: colors.accent, fontWeight: '600', fontSize: 12 }}>
        {t('miniapps.spaceVersion', { chat: item.space_title, version: item.version })}
      </Text>
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 32, fontWeight: '600' }}
      >
        {item.title}
      </Text>
      {error || read.error || specs.error || home.error || cards.error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error ?? read.error ?? specs.error ?? home.error ?? cards.error}
        </Text>
      ) : null}
      {message ? (
        <Text accessibilityLiveRegion="polite" selectable style={{ color: colors.accent }}>
          {message}
        </Text>
      ) : null}
      {!manage ? (
        spec?.has_view ? (
          <MiniappView
            item={item}
            spec={spec}
            onDocumentController={registerDocument}
            onDocumentCopied={openCopy}
            onDocumentDiscard={discardDocument}
          />
        ) : (
          <MiniappState app={app} busy={busy} onCall={call} />
        )
      ) : null}
      <MiniappSurfaceControls item={item} />
      {tools.some((tool) => !tool.read_only) ? (
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
                      router.push({
                        pathname: '/miniapp/[id]/tool',
                        params: { id, tool: tool.name },
                      })
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
      ) : null}
      <NativeButton
        secondary
        label={t('miniapps.thisMiniappSPermissions')}
        onPress={() => router.push({ pathname: '/miniapp-permissions', params: { item: id } })}
      />
    </ScrollView>
  );
}
