import { Text, View } from 'react-native';
import { t } from '../i18n/index.ts';
import { NativeButton } from '../components/primitives';
import { useTheme } from '../theme';
import type { AgentControlSession, AgentControlState } from './agent-control-session';

export function AgentControlView({
  state,
  session,
  disabled = false,
}: {
  state: AgentControlState;
  session: AgentControlSession;
  disabled?: boolean;
}) {
  const { colors } = useTheme();
  const ticket = session.ticket();
  const retry = session.retryTicket();
  const status = state.status;
  return (
    <View testID="agent-lifecycle-control" style={{ gap: 10 }}>
      <Text
        testID="agent-lifecycle-status"
        accessibilityLiveRegion="polite"
        style={{ color: colors.secondary }}
      >
        {status === 'Active'
          ? t('agents.active')
          : status === 'Paused'
            ? t('agents.paused')
            : status === 'Revoked'
              ? t('agents.revoked')
              : t('agents.checkingStatus')}
      </Text>
      {state.pending ? (
        <Text
          testID="agent-lifecycle-unknown"
          accessibilityRole="alert"
          style={{ color: colors.secondary, lineHeight: 21 }}
        >
          {t('agents.lifecycleReconciling')}
        </Text>
      ) : null}
      {state.phase === 'failed' && !state.pending ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {t('agents.thisCouldNotBeCompleted')}
        </Text>
      ) : null}
      {ticket ? (
        <NativeButton
          testID="agent-lifecycle-change"
          secondary
          disabled={disabled}
          label={ticket.action === 'Pause' ? t('agents.pauseAgent') : t('agents.resumeAgent')}
          onPress={() => {
            if (!disabled) void session.change(ticket);
          }}
        />
      ) : null}
      {retry ? (
        <NativeButton
          testID="agent-lifecycle-retry"
          secondary
          disabled={disabled || state.busy}
          label={
            retry.action === 'Pause'
              ? t('agents.retryPauseAgent')
              : retry.action === 'Resume'
                ? t('agents.retryResumeAgent')
                : t('agents.retryRevokeAgent')
          }
          onPress={() => {
            if (!disabled) void session.retry(retry);
          }}
        />
      ) : null}
      {state.pending || state.phase === 'failed' ? (
        <NativeButton
          testID="agent-lifecycle-refresh"
          secondary
          disabled={disabled || state.busy || state.phase === 'loading'}
          label={t('agents.lifecycleRefresh')}
          onPress={() => {
            if (!disabled) void session.refresh();
          }}
        />
      ) : null}
    </View>
  );
}
