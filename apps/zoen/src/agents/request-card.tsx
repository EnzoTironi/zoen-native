import { t, formatLocale } from '../i18n/index.ts';
import { useState } from 'react';
import { router } from 'expo-router';
import { Pressable, Text, View } from 'react-native';
import type { AgentRequestDto } from '../core/generated/AgentRequestDto';
import type { RequestDecision } from '../core/generated/RequestDecision';
import { useClient } from '../core/provider';
import { money } from '../core/presentation';
import { Avatar, NativeButton, Surface } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useTheme } from '../theme';

const statuses = {
  Pending: t('agents.waitingForYourDecision'),
  Stale: t('agents.theContentHasChanged'),
  Approved: t('agents.approved'),
  Denied: t('agents.notAllowed'),
};
export function RequestCard({
  request,
  compact = false,
  onResolved,
  disabled = false,
}: {
  request: AgentRequestDto;
  compact?: boolean;
  onResolved?: () => void;
  disabled?: boolean;
}) {
  const { store } = useClient();
  const dialogs = useDialogs();
  const { colors } = useTheme();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const pending = request.status === 'Pending' || request.status === 'Stale';
  const canDecide = pending && request.agent.is_mine && !busy && !disabled;
  async function decide(decision: RequestDecision) {
    if (!canDecide) return;
    const always = decision === 'AlwaysApprove' || decision === 'AlwaysDeny';
    if (
      always &&
      !(await dialogs.confirm({
        title:
          decision === 'AlwaysApprove'
            ? t('agents.alwaysAllowThisTypeOfAction')
            : t('agents.alwaysDenyThisTypeOfAction'),
        message: t('agents.standingDecisionConfirmation', {
          name: request.agent.name,
          chat: request.space_title,
          action: request.action_label,
        }),
        confirmLabel:
          decision === 'AlwaysApprove' ? t('agents.alwaysAllow') : t('agents.alwaysDeny'),
        destructive: decision === 'AlwaysDeny',
      }))
    )
      return;
    setBusy(true);
    setFailure(null);
    setResult(null);
    try {
      const outcome = await store.request({
        op: 'decide_request',
        request_id: request.id,
        decision,
      });
      setResult(
        outcome.also_resolved
          ? t('agents.alsoResolved', { message: outcome.message, count: outcome.also_resolved })
          : outcome.message,
      );
      onResolved?.();
    } catch (reason) {
      setFailure(
        reason instanceof Error ? reason.message : t('agents.thisDecisionCouldNotBeSaved'),
      );
    } finally {
      setBusy(false);
    }
  }
  return (
    <Surface>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={t('agents.viewAgentLabel', { name: request.agent.name })}
        onPress={() => router.push({ pathname: '/agent/[id]', params: { id: request.agent.id } })}
        style={{ flexDirection: 'row', alignItems: 'center', gap: 12, minHeight: 44 }}
      >
        <Avatar name={request.agent.name} color={request.agent.tint_hex} agent size={40} />
        <View style={{ flex: 1, gap: 4 }}>
          <Text style={{ color: colors.text, fontWeight: '600' }}>
            {request.agent.name} ·{' '}
            {request.agent.is_mine
              ? t('agents.yourAgent2')
              : t('agents.agentOwnedBy', {
                  name: request.agent.owner_name ?? t('agents.someoneElse'),
                })}
          </Text>
          <Text style={{ color: colors.secondary, fontSize: 12 }}>
            {request.space_title} ·{' '}
            {new Date(Number(request.opened_ms)).toLocaleString(formatLocale)}
          </Text>
        </View>
      </Pressable>
      <Text
        accessibilityRole="header"
        selectable
        style={{ color: colors.text, fontSize: 23, lineHeight: 29, fontWeight: '600' }}
      >
        {request.title}
      </Text>
      <Text
        selectable
        numberOfLines={compact ? 4 : undefined}
        style={{ color: colors.secondary, fontSize: 16, lineHeight: 24 }}
      >
        {request.detail}
      </Text>
      {!compact ? (
        <View style={{ gap: 8 }}>
          <Text selectable style={{ color: colors.secondary }}>
            {t('agents.requestAudience', { audience: request.audience })}
          </Text>
          <Text selectable style={{ color: colors.secondary }}>
            {t('agents.requestAction', { action: request.action_label })}
          </Text>
          {request.cost_cents === null ? null : (
            <Text
              selectable
              style={{
                color: colors.text,
                fontSize: 21,
                fontWeight: '600',
                fontVariant: ['tabular-nums'],
              }}
            >
              {money(request.cost_cents)}
            </Text>
          )}
          <Text selectable style={{ color: colors.secondary, fontSize: 14, lineHeight: 21 }}>
            {request.reason}
          </Text>
        </View>
      ) : request.cost_cents !== null ? (
        <Text selectable style={{ color: colors.text, fontWeight: '600' }}>
          {money(request.cost_cents)}
        </Text>
      ) : null}
      <Text
        accessibilityLiveRegion="polite"
        style={{
          color: request.status === 'Stale' ? colors.danger : colors.secondary,
          fontWeight: '600',
        }}
      >
        {request.by_standing
          ? t('agents.standingDecisionStatus', { status: statuses[request.status] })
          : statuses[request.status]}
      </Text>
      {request.status === 'Stale' ? (
        <Text style={{ color: colors.danger, lineHeight: 21 }}>
          {t('agents.thePreviousApprovalDoesNotMatchTheCurrent')}
        </Text>
      ) : pending ? (
        <Text style={{ color: colors.secondary, fontSize: 12, lineHeight: 18 }}>
          {t('agents.permissionAppliesToTheExactContentOfThis')}
        </Text>
      ) : null}
      {failure ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {failure}
        </Text>
      ) : null}
      {result ? (
        <Text accessibilityLiveRegion="polite" selectable style={{ color: colors.accent }}>
          {result}
        </Text>
      ) : null}
      <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 10 }}>
        {compact ? (
          <NativeButton
            secondary
            label={t('agents.reviewRequest')}
            disabled={busy}
            onPress={() => router.push({ pathname: '/request/[id]', params: { id: request.id } })}
          />
        ) : null}
        {request.item_id ? (
          <NativeButton
            secondary
            label={t('agents.openItem')}
            disabled={busy}
            onPress={() =>
              router.push({ pathname: '/item/[id]', params: { id: request.item_id! } })
            }
          />
        ) : null}
        {pending ? (
          <>
            <NativeButton
              secondary
              label={t('agents.doNotAllow')}
              disabled={!canDecide}
              onPress={() => {
                void decide('Deny');
              }}
            />
            <NativeButton
              label={busy ? t('agents.saving') : t('agents.allowOnce')}
              disabled={!canDecide || request.status === 'Stale'}
              onPress={() => {
                void decide('Approve');
              }}
            />
          </>
        ) : null}
      </View>
      {!compact && pending && request.agent.is_mine ? (
        <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 10, paddingTop: 6 }}>
          {request.can_always_approve ? (
            <NativeButton
              secondary
              label={t('agents.alwaysAllowThisType')}
              disabled={!canDecide || request.status === 'Stale'}
              onPress={() => {
                void decide('AlwaysApprove');
              }}
            />
          ) : (
            <Text style={{ color: colors.secondary, fontSize: 12, lineHeight: 18 }}>
              {t('agents.thisActionAlwaysRequiresAnIndividualDecision')}
            </Text>
          )}
          <NativeButton
            secondary
            label={t('agents.alwaysDenyThisType')}
            disabled={!canDecide}
            onPress={() => {
              void decide('AlwaysDeny');
            }}
          />
        </View>
      ) : null}
      {pending && !request.agent.is_mine ? (
        <Text style={{ color: colors.secondary }}>
          {t('agents.thisAgentSOwnerDecidesThisRequest')}
        </Text>
      ) : null}
    </Surface>
  );
}
