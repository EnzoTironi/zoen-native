import { Text, View } from 'react-native';
import { router } from 'expo-router';
import { t } from '../i18n/index.ts';
import { NativeButton } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useTheme } from '../theme';
import { useToolReview } from './use-tool-review';

export function ToolReviewRecovery({
  requestId,
  onResolved,
  disabled = false,
}: {
  requestId: string;
  onResolved?: () => void;
  disabled?: boolean;
}) {
  const { state, session } = useToolReview(requestId);
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const review = state.request?.tool_approval?.review;
  const ticket = disabled ? null : session.ticket();
  const successor = disabled ? null : session.successor();
  const message = state.unknown
    ? t('reviewRenewal.unknown')
    : state.phase === 'failed'
      ? t('reviewRenewal.failed')
      : state.phase !== 'ready'
        ? t('reviewRenewal.waiting')
        : review?.state === 'Requested'
          ? t('reviewRenewal.requested')
          : review?.state === 'Superseded'
            ? t('reviewRenewal.superseded')
            : review?.state === 'Reconcile'
              ? t('reviewRenewal.reconcile')
              : ticket
                ? t('reviewRenewal.expired')
                : t('reviewRenewal.unavailable');
  return (
    <View style={{ gap: 10 }}>
      <Text accessibilityLiveRegion="polite" style={{ color: colors.secondary, lineHeight: 21 }}>
        {message}
      </Text>
      {ticket ? (
        <NativeButton
          secondary
          label={t('reviewRenewal.renew')}
          disabled={state.busy || disabled}
          onPress={() => {
            if (disabled) return;
            void session
              .renew(ticket, () =>
                dialogs.confirm({
                  title: t('reviewRenewal.confirmationTitle'),
                  message: t('reviewRenewal.confirmation'),
                  confirmLabel: t('reviewRenewal.renew'),
                }),
              )
              .then((changed) => {
                if (changed) onResolved?.();
              });
          }}
        />
      ) : null}
      {successor && state.request?.agent.is_mine && !state.unknown ? (
        <NativeButton
          secondary
          label={t('reviewRenewal.open')}
          disabled={state.busy || disabled}
          onPress={() => {
            if (!disabled && session.successor() === successor)
              router.push({ pathname: '/request/[id]', params: { id: successor } });
          }}
        />
      ) : null}
      {state.phase === 'failed' || state.unknown || review?.state === 'Requested' ? (
        <NativeButton
          secondary
          label={t('reviewRenewal.refresh')}
          disabled={state.busy || disabled}
          onPress={() => {
            void session.refresh();
          }}
        />
      ) : null}
    </View>
  );
}
