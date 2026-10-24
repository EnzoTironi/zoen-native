import { t, formatLocale } from '../i18n/index.ts';
import { useCallback, useRef, useState } from 'react';
import { router } from 'expo-router';
import { Pressable, Text, View } from 'react-native';
import type { AgentRequestDto } from '../core/generated/AgentRequestDto';
import {
  requestDecisionCommand,
  toolApprovalExpired,
  toolApprovalValid,
  type RequestCardDecision,
} from '../core/tool-request-decision';
import { useClient } from '../core/provider';
import { money } from '../core/presentation';
import { Avatar, NativeButton, Surface } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { ToolReviewRecovery } from './tool-review-recovery';
import { SiteToolReview } from './site-tool-review';
import { siteScope, siteAuthority } from '../sites/source';
import { useTheme } from '../theme';
import { imageToolReviewValid } from '../core/image-tool-review';

const statuses = {
  Pending: t('agents.waitingForYourDecision'),
  Stale: t('agents.theContentHasChanged'),
  Approved: t('agents.approved'),
  Denied: t('agents.notAllowed'),
  Cancelled: t('agents.requestCancelled'),
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
  const deciding = useRef(false);
  const tool = request.tool_approval;
  const siteNeedsReview = tool?.preview_kind === 'site';
  const siteContext = siteNeedsReview ? siteScope(store, request.space_id) : null;
  const siteOwner = siteContext && siteAuthority(store, siteContext);
  const siteReviewKey = `${request.id}:${tool?.content_hash ?? ''}:${siteOwner ?? 'unavailable'}`;
  const [reviewedSiteKey, setReviewedSiteKey] = useState<string | null>(null);
  const siteReviewed = !!siteOwner && reviewedSiteKey === siteReviewKey;
  const siteReady = useCallback(
    (ready: boolean) => setReviewedSiteKey(ready ? siteReviewKey : null),
    [siteReviewKey],
  );
  const imageRequest = tool?.canonical_tool === 'generate_image';
  const image = imageRequest && imageToolReviewValid(tool.image) ? tool.image : null;
  const toolValid = tool == null || toolApprovalValid(tool);
  const toolExpired = tool != null && toolApprovalExpired(tool);
  const itemId = imageRequest
    ? null
    : tool == null
      ? request.item_id
      : toolValid
        ? tool.artifact_id
        : null;
  const historicalToolApproval = tool != null && toolValid && request.status === 'Approved';
  const pending = request.status === 'Pending' || request.status === 'Stale';
  const canDecide = pending && toolValid && request.agent.is_mine && !busy && !disabled;
  async function decide(decision: RequestCardDecision) {
    if (
      !canDecide ||
      deciding.current ||
      (decision === 'ApproveOnce' && siteNeedsReview && !siteReviewed)
    )
      return;
    const command = requestDecisionCommand(request, decision);
    if (!command) return;
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
    deciding.current = true;
    setBusy(true);
    setFailure(null);
    setResult(null);
    try {
      const outcome = await store.request(command);
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
      deciding.current = false;
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
        {image?.prompt ?? request.detail}
      </Text>
      {image ? (
        <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
          {[
            image.size.replace('x', ' × '),
            image.quality === 'low'
              ? t('imageReview.low')
              : image.quality === 'medium'
                ? t('imageReview.medium')
                : t('imageReview.high'),
            image.background === 'transparent'
              ? t('imageReview.transparent')
              : t('imageReview.opaque'),
          ].map((value) => (
            <Text
              key={value}
              selectable
              style={{ color: colors.secondary, fontSize: 14, lineHeight: 21 }}
            >
              {value}
            </Text>
          ))}
        </View>
      ) : null}
      {imageRequest && !image ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 21 }}>
          {t('imageReview.unavailable')}
        </Text>
      ) : null}
      {tool != null && toolValid && !imageRequest ? (
        <View style={{ gap: 6 }}>
          <Text selectable style={{ color: colors.text, fontWeight: '600', lineHeight: 22 }}>
            {t('agents.toolArtifactVersion', {
              title: tool.artifact_title,
              version: tool.version,
            })}
          </Text>
          <Text selectable style={{ color: colors.secondary, lineHeight: 21 }}>
            {t('agents.requestAction', { action: tool.operation })}
          </Text>
          {toolValid ? (
            <Text selectable style={{ color: colors.secondary, fontSize: 12, lineHeight: 18 }}>
              {t('agents.toolApprovalExpires', {
                time: new Date(Number(tool.expires_ms)).toLocaleString(formatLocale),
              })}
            </Text>
          ) : null}
        </View>
      ) : null}
      {siteNeedsReview && !compact && pending && tool && toolValid ? (
        <SiteToolReview
          key={`${request.id}:${tool.content_hash}`}
          requestId={request.id}
          contentHash={tool.content_hash}
          spaceId={request.space_id}
          onReady={siteReady}
        />
      ) : null}
      {!compact ? (
        <View style={{ gap: 8 }}>
          <Text selectable style={{ color: colors.secondary }}>
            {t('agents.requestAudience', { audience: request.audience })}
          </Text>
          {tool == null ? (
            <Text selectable style={{ color: colors.secondary }}>
              {t('agents.requestAction', { action: request.action_label })}
            </Text>
          ) : null}
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
        {historicalToolApproval
          ? t('reviewRenewal.historicalApproval')
          : tool == null && request.by_standing
            ? t('agents.standingDecisionStatus', { status: statuses[request.status] })
            : statuses[request.status]}
      </Text>
      {!toolValid ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 21 }}>
          {t('agents.toolApprovalUnavailable')}
        </Text>
      ) : tool != null && pending && toolExpired ? (
        <Text style={{ color: colors.danger, lineHeight: 21 }}>
          {t('agents.toolApprovalExpired')}
        </Text>
      ) : request.status === 'Stale' ? (
        <Text style={{ color: colors.danger, lineHeight: 21 }}>
          {t('agents.thePreviousApprovalDoesNotMatchTheCurrent')}
        </Text>
      ) : pending ? (
        <Text style={{ color: colors.secondary, fontSize: 12, lineHeight: 18 }}>
          {t('agents.permissionAppliesToTheExactContentOfThis')}
        </Text>
      ) : null}
      {historicalToolApproval ? (
        <ToolReviewRecovery
          requestId={request.id}
          onResolved={onResolved}
          disabled={disabled || busy}
        />
      ) : null}
      {failure ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {failure}
        </Text>
      ) : null}
      {result && !historicalToolApproval ? (
        <Text accessibilityLiveRegion="polite" selectable style={{ color: colors.accent }}>
          {result}
        </Text>
      ) : null}
      <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 10 }}>
        {!compact && image && pending && request.agent.is_mine ? (
          <NativeButton
            secondary
            label={t('ownerPolicy.title')}
            disabled={disabled || busy || toolExpired}
            onPress={() =>
              router.push({ pathname: '/owner-budget/[id]', params: { id: request.id } })
            }
          />
        ) : null}
        {compact ? (
          <NativeButton
            secondary
            label={t('agents.reviewRequest')}
            disabled={busy}
            onPress={() => router.push({ pathname: '/request/[id]', params: { id: request.id } })}
          />
        ) : null}
        {itemId && !itemId.startsWith('future_site_') ? (
          <NativeButton
            secondary
            label={t('agents.openItem')}
            disabled={busy}
            onPress={() => router.push({ pathname: '/item/[id]', params: { id: itemId } })}
          />
        ) : null}
        {pending ? (
          <>
            {tool != null ? (
              <NativeButton
                secondary
                label={t('agents.cancelRequest')}
                disabled={!canDecide}
                onPress={() => {
                  void decide('Cancel');
                }}
              />
            ) : null}
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
              disabled={
                !canDecide ||
                request.status === 'Stale' ||
                toolExpired ||
                (siteNeedsReview && (!siteReviewed || compact)) ||
                (imageRequest && !image)
              }
              onPress={() => {
                void decide(tool == null ? 'Approve' : 'ApproveOnce');
              }}
            />
          </>
        ) : null}
      </View>
      {!compact && pending && request.agent.is_mine && tool == null ? (
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
