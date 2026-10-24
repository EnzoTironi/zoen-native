import { Stack } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import type { AgentRequestDto } from '../core/generated/AgentRequestDto';
import type { PolicyTarget } from '../owner-policy/session';
import { useOwnerPolicy } from '../owner-policy/use-owner-policy';
import { EmptyState, Loading, NativeButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { OwnerPolicyForm } from './owner-policy-form';
import { OwnerPolicyReview, type OwnerPolicyReviewPhase } from './owner-policy-review';

export function OwnerPolicyScreen({ request }: { request: AgentRequestDto }) {
  return (
    <OwnerPolicyControllerScreen
      target={{
        spaceId: request.space_id,
        agentId: request.agent.id,
        requestId: request.id,
        expectedHash: request.tool_approval?.content_hash ?? '',
      }}
    />
  );
}
export function OwnerPolicyControllerScreen({ target }: { target: PolicyTarget }) {
  const { colors } = useTheme();
  const { session, state, unavailableReason } = useOwnerPolicy(target);
  if (state.phase === 'unavailable')
    return (
      <EmptyState
        title={
          unavailableReason === 'private-conversation-required'
            ? t('ownerPolicy.privateConversation')
            : t('ownerPolicy.unavailable')
        }
      />
    );
  const busy = state.refreshing || state.phase === 'submitting' || state.phase === 'preparing';
  const canRetryOptions = session.canRetryOptions(state);
  const context = state.context;
  const review = state.review;
  const reviewPhase: OwnerPolicyReviewPhase =
    state.phase === 'review'
      ? 'ready'
      : state.phase === 'refused'
        ? 'refused'
        : state.phase === 'installed'
          ? state.effectiveness === 'current'
            ? 'installed'
            : state.effectiveness === 'superseded'
              ? 'superseded'
              : state.effectiveness === 'expired'
                ? 'expired'
                : state.effectiveness === 'disabled'
                  ? 'disabled'
                  : 'historical'
          : 'pending';
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 20,
        maxWidth: 740,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: t('ownerPolicy.title') }} />
      {state.phase === 'loading' || state.phase === 'preparing' ? <Loading /> : null}
      {context && state.phase === 'editing' ? (
        <OwnerPolicyForm
          key={`${state.scope}:${context.options_digest}`}
          context={context}
          initial={{
            limit_units: state.limitInput,
            max_attempt_units: state.maxAttemptInput,
            enabled: state.enabled,
          }}
          active
          busy={busy}
          onPrepare={(draft) => {
            if (session.getSnapshot() !== state) return;
            session.edit(state, {
              limitInput: draft.limit_units,
              maxAttemptInput: draft.max_attempt_units,
              enabled: draft.enabled,
            });
            const next = session.getSnapshot();
            if (
              next.scope === state.scope &&
              next.phase === 'editing' &&
              next.context?.options_digest === context.options_digest &&
              next.limitInput === draft.limit_units &&
              next.maxAttemptInput === draft.max_attempt_units &&
              next.enabled === draft.enabled
            )
              void session.prepare(next);
          }}
        />
      ) : null}
      {review ? (
        <OwnerPolicyReview
          review={review}
          profiles={review.allowed_profiles.map((digest) => ({
            digest,
            label:
              digest === context?.image_profile_digest
                ? t('ownerPolicy.imageService')
                : t('ownerPolicy.previousService'),
          }))}
          phase={reviewPhase}
          active={state.phase === 'review'}
          busy={busy}
          onConfirm={() => {
            const ticket = session.confirmation(state);
            if (ticket) void session.submit(ticket);
          }}
        />
      ) : null}
      {state.error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {t('ownerPolicy.operationFailed')}
        </Text>
      ) : null}
      {state.phase === 'held' ? (
        <Text accessibilityLiveRegion="polite" selectable style={{ color: colors.secondary }}>
          {t('ownerPolicy.uncertain')}
        </Text>
      ) : null}
      {!review && ['refused', 'delivery_failed'].includes(state.optionsState ?? '') ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {t('ownerPolicy.quoteUnavailable')}
        </Text>
      ) : null}
      {state.phase === 'pending' && !review ? (
        <Text accessibilityLiveRegion="polite" style={{ color: colors.secondary }}>
          {t('ownerPolicy.waitingForQuote')}
        </Text>
      ) : null}
      <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 12 }}>
        {state.phase === 'review' ? (
          <NativeButton
            secondary
            label={t('ownerPolicy.edit')}
            disabled={busy}
            onPress={() => session.edit(state, {})}
          />
        ) : null}
        {['held', 'pending'].includes(state.phase) ||
        (state.phase === 'loading' && state.optionsSource) ? (
          <NativeButton
            secondary
            label={t('ownerPolicy.checkStatus')}
            disabled={busy}
            onPress={() => {
              void session.reconcile(state);
            }}
          />
        ) : null}
        {state.receipt && ['installed', 'refused'].includes(state.phase) ? (
          <NativeButton
            secondary
            label={t('ownerPolicy.edit')}
            disabled={busy || !context}
            onPress={() => {
              void session.beginEdit(state);
            }}
          />
        ) : null}
        {canRetryOptions ? (
          <NativeButton
            secondary
            label={t('agents.tryAgain')}
            disabled={busy}
            onPress={() => {
              void session.retryOptions(state);
            }}
          />
        ) : null}
        {state.error && !canRetryOptions && state.phase !== 'held' && state.phase !== 'pending' ? (
          <NativeButton
            secondary
            label={t('agents.tryAgain')}
            disabled={busy}
            onPress={() => {
              void session.refresh(state);
            }}
          />
        ) : null}
      </View>
    </ScrollView>
  );
}
