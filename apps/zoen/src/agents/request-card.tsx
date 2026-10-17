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
  Pending: 'Aguardando sua decisão',
  Stale: 'O conteúdo mudou',
  Approved: 'Aprovado',
  Denied: 'Não permitido',
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
            ? 'Sempre permitir este tipo de ação?'
            : 'Sempre negar este tipo de ação?',
        message: `${request.agent.name} · ${request.space_title}\n${request.action_label}\n\nA decisão também vale para pedidos atuais compatíveis e para os próximos deste agente nesta conversa. Você pode revogá-la nas permissões do agente.`,
        confirmLabel: decision === 'AlwaysApprove' ? 'Sempre permitir' : 'Sempre negar',
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
        `${outcome.message}${outcome.also_resolved ? ` · ${outcome.also_resolved} outro(s) pedido(s) resolvido(s).` : ''}`,
      );
      onResolved?.();
    } catch (reason) {
      setFailure(
        reason instanceof Error ? reason.message : 'Não foi possível guardar esta decisão.',
      );
    } finally {
      setBusy(false);
    }
  }
  return (
    <Surface>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={`Ver agente ${request.agent.name}`}
        onPress={() => router.push({ pathname: '/agent/[id]', params: { id: request.agent.id } })}
        style={{ flexDirection: 'row', alignItems: 'center', gap: 12, minHeight: 44 }}
      >
        <Avatar name={request.agent.name} color={request.agent.tint_hex} agent size={40} />
        <View style={{ flex: 1, gap: 4 }}>
          <Text style={{ color: colors.text, fontWeight: '600' }}>
            {request.agent.name} ·{' '}
            {request.agent.is_mine
              ? 'seu agente'
              : `agente de ${request.agent.owner_name ?? 'outra pessoa'}`}
          </Text>
          <Text style={{ color: colors.secondary, fontSize: 12 }}>
            {request.space_title} · {new Date(Number(request.opened_ms)).toLocaleString('pt-BR')}
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
            Quem vê: {request.audience}
          </Text>
          <Text selectable style={{ color: colors.secondary }}>
            Ação: {request.action_label}
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
        {statuses[request.status]}
        {request.by_standing ? ' · decisão permanente' : ''}
      </Text>
      {request.status === 'Stale' ? (
        <Text style={{ color: colors.danger, lineHeight: 21 }}>
          A aprovação anterior não corresponde ao conteúdo atual. Confira o item e peça uma nova
          proposta antes de permitir.
        </Text>
      ) : pending ? (
        <Text style={{ color: colors.secondary, fontSize: 12, lineHeight: 18 }}>
          A permissão vale para o conteúdo exato deste pedido. Se ele mudar, será preciso revisar
          novamente.
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
            label="Revisar pedido"
            disabled={busy}
            onPress={() => router.push({ pathname: '/request/[id]', params: { id: request.id } })}
          />
        ) : null}
        {request.item_id ? (
          <NativeButton
            secondary
            label="Abrir item"
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
              label="Não permitir"
              disabled={!canDecide}
              onPress={() => {
                void decide('Deny');
              }}
            />
            <NativeButton
              label={busy ? 'Guardando…' : 'Permitir uma vez'}
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
              label="Sempre permitir este tipo"
              disabled={!canDecide || request.status === 'Stale'}
              onPress={() => {
                void decide('AlwaysApprove');
              }}
            />
          ) : (
            <Text style={{ color: colors.secondary, fontSize: 12, lineHeight: 18 }}>
              Esta ação sempre pede uma decisão individual.
            </Text>
          )}
          <NativeButton
            secondary
            label="Sempre negar este tipo"
            disabled={!canDecide}
            onPress={() => {
              void decide('AlwaysDeny');
            }}
          />
        </View>
      ) : null}
      {pending && !request.agent.is_mine ? (
        <Text style={{ color: colors.secondary }}>
          O dono deste agente decide esta solicitação.
        </Text>
      ) : null}
    </Surface>
  );
}
