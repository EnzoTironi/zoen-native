import * as Crypto from 'expo-crypto';
import { BottomSheet, Host, RNHostView } from '@expo/ui';
import { useEffect, useMemo, useSyncExternalStore } from 'react';
import { AppState, ScrollView, Text, View, useWindowDimensions } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { RequestCard } from '../agents/request-card';
import { IconButton, Loading, NativeButton } from '../components/primitives';
import { MessageMarkdown } from '../components/message-markdown';
import { ComposerTextField } from '../components/composer-text-field';
import { useClient } from '../core/provider';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { useAppSheetCloseButton } from './app-sheet-context';
import type { MiniappDocumentController } from './document-controller';
import { editorAgentPort, editorAgentScope } from './editor-agent-port';
import { EditorAgentSession } from './editor-agent-session';

/** A state-presented native sheet; the sibling editor/route never changes identity. */
export function EditorAgentAccess({
  itemId,
  spaceId,
  document,
}: {
  itemId: string;
  spaceId: string;
  document: MiniappDocumentController;
}) {
  const { store } = useClient();
  const { scheme, colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const chrome = useAppSheetCloseButton();
  const owner = editorAgentScope(store, spaceId);
  const session = useMemo(
    () =>
      new EditorAgentSession(
        editorAgentPort(store, itemId, spaceId, document, owner, () => Crypto.randomUUID()),
        owner,
        itemId,
        spaceId,
      ),
    [store, itemId, spaceId, document, owner],
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  useEffect(() => {
    session.activate();
    const stop = store.subscribe(() => {
      if (editorAgentScope(store, spaceId) !== owner) session.retire();
    });
    const changed = store.subscribeCanonicalChanges(() => {
      void session.refresh();
    });
    const app = AppState.addEventListener('change', (next) => {
      if (next !== 'active') session.close();
    });
    return () => {
      stop();
      changed();
      app.remove();
      session.retire();
    };
  }, [store, spaceId, owner, session]);
  return (
    <>
      <View
        style={{
          position: 'absolute',
          top: chrome?.top ?? insets.top + 8,
          right: (chrome?.right ?? insets.right + 12) + (chrome?.width ?? 44) + 8,
          borderRadius: 24,
          backgroundColor: colors.surface,
        }}
      >
        <IconButton
          testID="editor-agent-open"
          name={{ ios: 'sparkles', android: 'auto_awesome', web: 'auto_awesome' }}
          label={t('miniapps.askAgent')}
          disabled={!owner || state.visible}
          onPress={session.open}
        />
      </View>
      <Host
        style={{ position: 'absolute' }}
        pointerEvents="none"
        colorScheme={scheme}
        seedColor={colors.accent}
      >
        <BottomSheet
          testID="editor-agent-sheet"
          isPresented={state.visible}
          onDismiss={session.close}
          snapPoints={['half', 'full']}
          containerColor={colors.background}
        >
          <RNHostView>
            <ScrollView
              contentInsetAdjustmentBehavior="automatic"
              onAccessibilityEscape={session.close}
              keyboardShouldPersistTaps="handled"
              automaticallyAdjustKeyboardInsets
              contentContainerStyle={{ padding: 20, gap: 16, paddingBottom: 32 }}
            >
              <View style={{ alignItems: 'flex-end' }}>
                <IconButton
                  name={{ ios: 'xmark', android: 'close', web: 'close' }}
                  label={t('common.close')}
                  onPress={session.close}
                />
              </View>
              <ComposerTextField
                context={`editor-agent:${itemId}`}
                testID="editor-agent-prompt"
                accessibilityLabel={t('miniapps.editRequestPlaceholder')}
                placeholder={t('miniapps.editRequestPlaceholder')}
                value={state.draft}
                onChangeText={session.setDraft}
                multiline
                maxLength={4000}
                editable={!state.busy && !state.uncertain}
                style={{
                  minHeight: 72,
                  fontSize: 17,
                  color: colors.text,
                  backgroundColor: colors.surface,
                  borderRadius: 12,
                  padding: 14,
                }}
              />
              <NativeButton
                testID="editor-agent-send"
                label={state.uncertain ? t('miniapps.tryAgain') : t('miniapps.askAgent')}
                disabled={state.busy || !state.draft.trim()}
                onPress={() => {
                  void session.send();
                }}
              />
              {state.uncertain ? (
                <Text selectable style={{ color: colors.secondary }}>
                  {t('miniapps.editRequestUncertain')}
                </Text>
              ) : null}
              {state.error ? (
                <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
                  {state.error.startsWith('editor_agent_')
                    ? t('miniapps.theMiniappCouldNotCompleteThisAction')
                    : state.error}
                </Text>
              ) : null}
              {state.sent ? (
                <Text
                  accessibilityLiveRegion="polite"
                  selectable
                  style={{ color: colors.secondary }}
                >
                  {t('miniapps.editRequestSent')}
                </Text>
              ) : null}
              {state.sent && 'Message' in state.sent.kind ? (
                <MessageMarkdown
                  text={state.sent.kind.Message.text}
                  color={colors.secondary}
                  fontSize={16}
                  maxWidth={Math.max(180, width - 40)}
                  selectable
                />
              ) : null}
              {state.replies.map((entry) =>
                'Message' in entry.kind ? (
                  <View key={entry.id} style={{ gap: 6 }}>
                    <Text style={{ color: colors.secondary, fontWeight: '600' }}>
                      {entry.author.name}
                    </Text>
                    <MessageMarkdown
                      text={entry.kind.Message.text}
                      color={colors.text}
                      fontSize={17}
                      maxWidth={Math.max(180, width - 40)}
                      selectable
                    />
                  </View>
                ) : null,
              )}
              {state.loading ? <Loading /> : null}
              {state.requests.map((request) => (
                <RequestCard
                  key={request.id}
                  request={request}
                  embedded
                  disabled={state.busy}
                  onResolved={() => {
                    void session.refresh();
                  }}
                  onDecision={(command) => {
                    if (command.op !== 'decide_tool_request')
                      return Promise.reject(Error('editor_agent_review_changed'));
                    return session.decide(command);
                  }}
                />
              ))}
            </ScrollView>
          </RNHostView>
        </BottomSheet>
      </Host>
    </>
  );
}
