import { binaryEditorFormat, isBinaryEditorId } from './binary-formats';
import { selectedDeckHead } from './deck-version-selection';
import { t } from '../i18n/index.ts';
import { useCallback, useMemo, useRef, useState } from 'react';
import { Stack, router, useLocalSearchParams, usePreventRemove } from 'expo-router';
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
import { EditorAgentAccess } from './editor-agent-sheet';
import type { MiniappDocumentController } from './document-controller';

export function MiniappScreen({ manage = false }: { manage?: boolean }) {
  const { id, version, sha256 } = useLocalSearchParams<{
    id: string;
    version?: string;
    sha256?: string;
  }>();
  const historical = version !== undefined || sha256 !== undefined;
  const selectedHead = useMemo(() => selectedDeckHead(version, sha256), [version, sha256]);
  const { store } = useClient();
  const { colors } = useTheme();
  const home = useHome();
  const dialogs = useDialogs();
  const read = useCoreQuery(
    historical
      ? selectedHead
        ? { op: 'item_at', item_id: id, version: Number(selectedHead.version) }
        : null
      : { op: 'item', item_id: id },
  );
  const specs = useCoreQuery({ op: 'app_specs' });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [document, setDocument] = useState<MiniappDocumentController | null>(null);
  const [leaving, setLeaving] = useState(false);
  const navigating = useRef(false);
  const editor = read.data?.app?.app_id;
  const documentNamespace = isBinaryEditorId(editor)
    ? binaryEditorFormat(editor).messages
    : 'drawing';
  const unavailable = isBinaryEditorId(editor)
    ? t(`${binaryEditorFormat(editor).messages}.unavailable`)
    : t('miniapps.theMiniappCouldNotBeOpened');
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
      .catch(() => setError(t(`${documentNamespace}.closeFailed`)))
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
        title: t(`${documentNamespace}.discard`),
        message: t(`${documentNamespace}.discardDetails`),
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
  }, [dialogs, disablePrevention, documentNamespace]);
  const item = read.data;
  const cards = useConversationCards(item?.space_id ?? '');
  const app = item?.app;
  if (historical && manage) return <EmptyState title={unavailable} />;
  if (historical && !selectedHead)
    return <EmptyState title={t('documents.thisVersionCouldNotBeLoaded')} />;
  if (historical && item && !isBinaryEditorId(item.app?.app_id))
    return <EmptyState title={unavailable} />;
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
  if (historical && !spec)
    return specs.error ? <EmptyState title={unavailable} detail={specs.error} /> : <Loading />;
  if (historical && !spec?.has_view) return <EmptyState title={unavailable} />;
  const tools =
    spec?.tools.filter(
      (tool) =>
        tool.visibility.includes('app') &&
        !(spec.id === 'excalidraw' && tool.name.startsWith('app_document_')) &&
        !(isBinaryEditorId(spec.id) && tool.name.startsWith('app_binary_')),
    ) ?? [];
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
        headerShown: manage,
        // Interactive canvases close through the guarded native close control.
        gestureEnabled: !fullscreen,
        headerRight: !manage
          ? undefined
          : () =>
              process.env.EXPO_OS === 'ios' ? (
                <HomeCardMenu
                  testID="miniapp-detail-options"
                  name={item.title}
                  pinned={home.isPinned(item.id)}
                  onToggle={
                    !home.loading && home.allItems.some((candidate) => candidate.id === item.id)
                      ? () => {
                          void (
                            home.isPinned(item.id) ? home.unpin(item.id) : home.pin(item.id)
                          ).catch(() => {});
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
                  onPress={() =>
                    router.push({ pathname: '/item/[id]', params: { id, history: '1' } })
                  }
                />
              ),
      }}
    />
  );
  if (fullscreen && spec) {
    return (
      <View
        testID="miniapp-fullscreen"
        onAccessibilityEscape={() => router.back()}
        style={{ flex: 1 }}
      >
        {navigation}
        {error || read.error || specs.error ? (
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger, padding: 16 }}>
            {error ?? read.error ?? specs.error}
          </Text>
        ) : null}
        <MiniappView
          item={item}
          selectedHead={selectedHead ?? undefined}
          spec={spec}
          fill
          onDocumentController={registerDocument}
          onDocumentCopied={openCopy}
          onDocumentDiscard={discardDocument}
        />
        {!historical &&
        isBinaryEditorId(spec.id) &&
        spec.id === 'wordcraft' &&
        document?.observedHead ? (
          <EditorAgentAccess itemId={item.id} spaceId={item.space_id} document={document} />
        ) : null}
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
