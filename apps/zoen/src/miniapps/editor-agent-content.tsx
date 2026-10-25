import { ScrollView, Text, View } from 'react-native';
import { RequestCard } from '../agents/request-card';
import { IconButton, Loading, NativeButton } from '../components/primitives';
import { MessageMarkdown } from '../components/message-markdown';
import { ComposerTextField } from '../components/composer-text-field';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type {
  EditorAgentSession,
  EditorAgentState,
  EditorAgentTicket,
} from './editor-agent-session';
import type { EditorAgentAdmission } from './editor-agent-admission';

export function EditorAgentContent({
  session,
  state,
  ticket,
  itemId,
  width,
  admission,
  hasAgents,
  onClose,
  onAdd,
  onRefresh,
  onSend,
}: {
  session: EditorAgentSession;
  state: EditorAgentState;
  ticket: EditorAgentTicket | null;
  itemId: string;
  width: number;
  admission: EditorAgentAdmission;
  hasAgents: boolean;
  onClose: () => void;
  onAdd: () => void;
  onRefresh: () => void;
  onSend: () => void;
}) {
  const { colors } = useTheme();
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      onAccessibilityEscape={onClose}
      keyboardShouldPersistTaps="handled"
      automaticallyAdjustKeyboardInsets
      contentContainerStyle={{ padding: 20, gap: 16, paddingBottom: 32 }}
    >
      <View style={{ alignItems: 'flex-end' }}>
        <IconButton
          name={{ ios: 'xmark', android: 'close', web: 'close' }}
          label={t('common.close')}
          onPress={onClose}
        />
      </View>
      <ComposerTextField
        context={`editor-agent:${itemId}`}
        testID="editor-agent-prompt"
        accessibilityLabel={t('miniapps.editRequestPlaceholder')}
        placeholder={t('miniapps.editRequestPlaceholder')}
        value={state.draft}
        onChangeText={(text) => session.setDraft(text, ticket)}
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
      {!state.uncertain ? (
        <Text style={{ color: colors.secondary }}>{t('miniapps.shareCurrentDisclosure')}</Text>
      ) : null}
      {!state.uncertain && admission === 'missing' ? (
        <NativeButton
          testID="editor-agent-add"
          secondary
          label={hasAgents ? t('miniapps.addOwnAgent') : t('agentEnrollment.connect')}
          onPress={onAdd}
        />
      ) : null}
      {!state.error && admission === 'waiting' ? (
        <Text accessibilityLiveRegion="polite" style={{ color: colors.secondary }}>
          {t('community.connecting')}
        </Text>
      ) : null}
      {admission === 'unavailable' ? (
        <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
          {t('miniapps.agentUnavailable')}
        </Text>
      ) : null}
      {admission === 'failed' ? (
        <NativeButton
          testID="editor-agent-admission-retry"
          secondary
          label={t('chat.tryAgain2')}
          onPress={onRefresh}
        />
      ) : null}
      <NativeButton
        testID="editor-agent-send"
        label={state.uncertain ? t('miniapps.tryAgain') : t('miniapps.sendEditRequest')}
        disabled={state.busy || !state.draft.trim() || (!state.uncertain && admission !== 'ready')}
        onPress={onSend}
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
          style={{ color: state.sent.delivery === 'Failed' ? colors.danger : colors.secondary }}
        >
          {t(
            state.sent.delivery === 'Failed'
              ? 'miniapps.editRequestFailed'
              : 'miniapps.editRequestSent',
          )}
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
            <Text style={{ color: colors.secondary, fontWeight: '600' }}>{entry.author.name}</Text>
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
            if (!session.accepts(ticket) || command.op !== 'decide_tool_request')
              return Promise.reject(Error('editor_agent_review_changed'));
            return session.decide(command);
          }}
        />
      ))}
    </ScrollView>
  );
}
