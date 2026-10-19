import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { ActivityIndicator, ScrollView, Text, View, type ScrollViewInstance } from 'react-native';
import { Stack, useLocalSearchParams } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { EphemeralConversation } from '../../assistant/conversation';
import type { AssistantUnavailableReason } from '../../assistant/types';
import { readAssistantLaunch, releaseAssistantLaunch } from '../../assistant/launch';
import {
  ConversationContainer,
  ConversationViewport,
} from '../../components/conversation-viewport';
import { MessageComposer } from '../../components/message-composer';
import { EmptyState, Loading, NativeButton } from '../../components/primitives';
import { useClient } from '../../core/provider';
import { locale, t } from '../../i18n/index.ts';
import { useTheme } from '../../theme';

function unavailable(reason: AssistantUnavailableReason | undefined): string {
  switch (reason) {
    case 'not_eligible':
      return t('agents.temporaryNotEligible');
    case 'disabled':
      return t('agents.temporaryDisabled');
    case 'not_ready':
      return t('agents.temporaryNotReady');
    case 'unsupported_locale':
      return t('agents.temporaryUnsupportedLocale');
    default:
      return t('agents.temporaryUnsupported');
  }
}

export default function AskZoen() {
  const { space, ticket } = useLocalSearchParams<{ space: string; ticket?: string }>();
  const { store } = useClient();
  const port = store.driver.assistant;
  const launch = useMemo(() => readAssistantLaunch(ticket, space), [ticket, space]);
  const [restart, setRestart] = useState(0);
  const [fresh, setFresh] = useState(false);
  const scope = `${space}:${ticket ?? ''}:${restart}`;
  const [owned, setOwned] = useState<{ scope: string; owner: EphemeralConversation } | null>(null);
  const [draft, setDraft] = useState('');
  const [inputError, setInputError] = useState(false);
  useEffect(() => () => releaseAssistantLaunch(ticket), [ticket]);
  useEffect(() => {
    if (!port) return;
    let active = true;
    const owner = new EphemeralConversation(port);
    const question = fresh ? '' : (launch?.query ?? '');
    let unregister = () => {};
    const close = async () => {
      await owner.close();
      unregister();
    };
    unregister = store.registerDeviceCleanup(close);
    void owner.open(space, locale).then(async () => {
      if (!active) return;
      setOwned({ scope, owner });
      setDraft(question);
      if (owner.getSnapshot().phase !== 'ready' || !question) return;
      if (new TextEncoder().encode(question).byteLength > 512) {
        setInputError(true);
        return;
      }
      setDraft('');
      const accepted = await owner.ask(question);
      if (active && !accepted) setDraft((current) => current || question);
    });
    return () => {
      active = false;
      void close().catch(() => {});
    };
  }, [port, space, scope, launch, fresh, store]);
  return (
    <>
      <Stack.Screen options={{ title: t('agents.temporaryChat') }} />
      {!port ? (
        <EmptyState title={t('agents.temporaryUnavailable')} detail={unavailable('unsupported')} />
      ) : !owned || owned.scope !== scope ? (
        <Loading />
      ) : (
        <Conversation
          owner={owned.owner}
          space={space}
          draft={draft}
          inputError={inputError}
          onDraft={(value) => {
            setDraft(value);
            setInputError(false);
          }}
          onTooLong={() => setInputError(true)}
          onRestoreDraft={(question) => setDraft((current) => current || question)}
          onRestart={() => {
            setInputError(false);
            setRestart((value) => value + 1);
          }}
          onNew={() => {
            setInputError(false);
            setFresh(true);
            setRestart((value) => value + 1);
          }}
        />
      )}
    </>
  );
}

function Conversation({
  owner,
  space,
  draft,
  inputError,
  onDraft,
  onTooLong,
  onRestoreDraft,
  onRestart,
  onNew,
}: {
  owner: EphemeralConversation;
  space: string;
  draft: string;
  inputError: boolean;
  onDraft: (value: string) => void;
  onTooLong: () => void;
  onRestoreDraft: (question: string) => void;
  onRestart: () => void;
  onNew: () => void;
}) {
  const state = useSyncExternalStore(owner.subscribe, owner.getSnapshot, owner.getSnapshot);
  const list = useRef<ScrollViewInstance>(null);
  useEffect(() => {
    const frame = requestAnimationFrame(() => list.current?.scrollToEnd({ animated: true }));
    return () => cancelAnimationFrame(frame);
  }, [state.turns.length, state.pending]);
  const { colors } = useTheme();
  const headerHeight = useHeaderHeight();
  const { top: statusBarHeight } = useSafeAreaInsets();
  const finished = state.turns.length >= 8;
  const turns = state.pending
    ? [...state.turns, { role: 'user' as const, text: state.pending }]
    : state.turns;
  async function send() {
    const question = draft.trim();
    if (state.phase !== 'ready' || finished || !question) return;
    if (new TextEncoder().encode(question).byteLength > 512) {
      onTooLong();
      return;
    }
    onDraft('');
    if (!(await owner.ask(question)) && owner.getSnapshot().phase !== 'closed')
      onRestoreDraft(question);
  }
  if (state.phase === 'opening') return <Loading label={t('agents.temporaryPreparing')} />;
  if (state.phase === 'unavailable')
    return (
      <EmptyState title={t('agents.temporaryUnavailable')} detail={unavailable(state.reason)} />
    );
  if (state.phase === 'failed')
    return (
      <EmptyState title={t('agents.temporaryUnavailable')} detail={t('agents.temporaryFailed')}>
        <NativeButton label={t('agents.tryAgain')} onPress={onRestart} />
      </EmptyState>
    );
  return (
    <ConversationContainer style={{ flex: 1 }}>
      <ConversationViewport
        composer={
          finished ? (
            <View style={{ padding: 16 }}>
              <NativeButton label={t('agents.temporaryNew')} onPress={onNew} />
            </View>
          ) : (
            <MessageComposer
              spaceId={space}
              value={draft}
              onChangeText={onDraft}
              onBlur={() => {}}
              onSend={() => {
                void send();
              }}
              sending={state.phase !== 'ready'}
              mediaEnabled={false}
            />
          )
        }
      >
        {(composerSpacer, NativeScrollComponent) => {
          const ScrollComponent = NativeScrollComponent ?? ScrollView;
          return (
            <ScrollComponent
              ref={list}
              testID="conversation-timeline"
              style={process.env.EXPO_OS === 'ios' ? { marginTop: statusBarHeight } : undefined}
              contentInsetAdjustmentBehavior="automatic"
              keyboardDismissMode="interactive"
              keyboardShouldPersistTaps="handled"
              maintainVisibleContentPosition={{
                minIndexForVisible: 0,
              }}
              contentContainerStyle={{
                paddingHorizontal: 20,
                paddingTop:
                  process.env.EXPO_OS === 'ios'
                    ? Math.max(0, headerHeight - statusBarHeight) + 20
                    : 20,
                paddingBottom: 12,
                gap: 16,
              }}
            >
              <View style={{ gap: 6, paddingBottom: 12 }}>
                <Text style={{ color: colors.accent, fontSize: 13, fontWeight: '600' }}>
                  {t('agents.temporaryPrivacy')}
                </Text>
                <Text style={{ color: colors.secondary, fontSize: 14, lineHeight: 21 }}>
                  {t('agents.temporaryContext')}
                </Text>
              </View>
              {turns.map((turn, index) => (
                <View
                  key={index}
                  style={{
                    alignSelf: turn.role === 'user' ? 'flex-end' : 'flex-start',
                    maxWidth: '92%',
                    borderRadius: 24,
                    padding: 16,
                    backgroundColor: turn.role === 'user' ? colors.mine : colors.theirs,
                  }}
                >
                  <Text
                    selectable
                    style={{
                      color: turn.role === 'user' ? colors.onMine : colors.text,
                      fontSize: 17,
                      lineHeight: 25,
                    }}
                  >
                    {turn.text}
                  </Text>
                </View>
              ))}
              {state.phase === 'asking' ? (
                <View accessibilityRole="progressbar" style={{ flexDirection: 'row', gap: 10 }}>
                  <ActivityIndicator color={colors.secondary} />
                  <Text style={{ color: colors.secondary }}>{t('agents.temporaryThinking')}</Text>
                </View>
              ) : null}
              {inputError || state.failed ? (
                <Text accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 22 }}>
                  {inputError ? t('agents.temporaryQuestionTooLong') : t('agents.temporaryFailed')}
                </Text>
              ) : null}
              {finished ? (
                <Text style={{ color: colors.secondary, lineHeight: 22 }}>
                  {t('agents.temporaryFinished')}
                </Text>
              ) : null}
              {composerSpacer}
            </ScrollComponent>
          );
        }}
      </ConversationViewport>
    </ConversationContainer>
  );
}
