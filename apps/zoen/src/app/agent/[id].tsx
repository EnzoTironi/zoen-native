import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { Pressable, ScrollView, Share, Text, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import { money, parseMoneyInput } from '../../core/presentation';
import type { TrustLevelDto } from '../../core/generated/TrustLevelDto';
import {
  Avatar,
  EmptyState,
  Field,
  Loading,
  NativeButton,
  Surface,
} from '../../components/primitives';
import { useTheme } from '../../theme';
import { useDialogs } from '../../components/dialogs';

const levels: { value: TrustLevelDto; title: string; detail: string }[] = [
  { value: 'Listen', title: 'Ouvir', detail: 'Acompanha a conversa.' },
  { value: 'Suggest', title: 'Sugerir', detail: 'Propõe mudanças e pede sua aprovação.' },
  {
    value: 'Act',
    title: 'Agir',
    detail: 'Executa ações permitidas, com possibilidade de desfazer.',
  },
  {
    value: 'Autonomous',
    title: 'Autônomo',
    detail: 'Age dentro das permissões e do orçamento que você definiu.',
  },
];
export default function Agent() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { store, mode } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const { data: agent, error, retry } = useCoreQuery({ op: 'agent_profile', agent_id: id });
  const standing = useCoreQuery({ op: 'standing_decisions' });
  const status = useCoreQuery(
    mode === 'account' && agent?.persona.is_mine ? { op: 'agent_status', agent_id: id } : null,
  );
  const activity = useCoreQuery({ op: 'agent_activity', agent_id: id });
  const [previewSpace, setPreviewSpace] = useState<string | null>(null);
  const selectedSpace = previewSpace ?? agent?.spaces[0]?.space_id;
  const previews = useCoreQuery(
    selectedSpace ? { op: 'preview_decisions', agent_id: id, space_id: selectedSpace } : null,
  );
  const [extra, setExtra] = useState('');
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  async function act(action: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    setMessage(null);
    try {
      await action();
      status.retry();
      retry();
      standing.retry();
      activity.retry();
      previews.retry();
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : 'Não foi possível concluir.');
    } finally {
      setBusy(false);
    }
  }
  if (!agent)
    return error ? (
      <EmptyState title="Agente indisponível" detail={error}>
        <NativeButton label="Tentar novamente" onPress={retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 22,
        maxWidth: 850,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: agent.persona.name }} />
      <View style={{ alignItems: 'center', gap: 14, paddingVertical: 18 }}>
        <Avatar name={agent.persona.name} color={agent.persona.tint_hex} size={88} agent />
        <Text style={{ color: colors.text, fontSize: 29, fontWeight: '600' }}>
          {agent.persona.name}
        </Text>
        <Text style={{ color: colors.secondary }}>{agent.runs_on}</Text>
        {agent.persona.bio ? (
          <Text selectable style={{ color: colors.secondary, lineHeight: 23, textAlign: 'center' }}>
            {agent.persona.bio}
          </Text>
        ) : null}
        <Text style={{ color: colors.secondary }}>
          {agent.persona.is_mine
            ? 'Você administra este agente.'
            : `Administrado por ${agent.persona.owner_name ?? 'outra pessoa'}.`}
        </Text>
        <NativeButton
          secondary
          label={
            agent.owner_notes_pending
              ? `Conversar · ${agent.owner_notes_pending} recado(s) aguardando`
              : 'Conversar com agente'
          }
          disabled={busy}
          onPress={() => {
            void act(async () => {
              const space = await store.request({ op: 'start_direct', identity_id: id });
              router.push({ pathname: '/chat/[id]', params: { id: space } });
            });
          }}
        />
      </View>
      {failure || error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {failure ?? error}
        </Text>
      ) : null}
      {message ? (
        <Text accessibilityLiveRegion="polite" selectable style={{ color: colors.accent }}>
          {message}
        </Text>
      ) : null}
      {agent.budget_limit_cents !== null ? (
        <Surface>
          <Text style={{ color: colors.secondary }}>Orçamento</Text>
          <Text
            selectable
            style={{
              color: colors.text,
              fontSize: 24,
              fontWeight: '600',
              fontVariant: ['tabular-nums'],
            }}
          >
            {agent.budget_spent_cents === null
              ? 'Consumo ainda indisponível'
              : `${money(agent.budget_spent_cents)} de ${money(agent.budget_limit_cents)}`}
          </Text>
          {agent.near_limit ? (
            <Text style={{ color: colors.danger }}>Perto do limite de orçamento.</Text>
          ) : null}
          {agent.persona.is_mine ? (
            <>
              <Field
                label="Adicionar orçamento (R$)"
                value={extra}
                onChangeText={setExtra}
                keyboardType="decimal-pad"
                placeholder="100,00"
                editable={!busy}
              />
              <NativeButton
                secondary
                disabled={busy || !extra.trim()}
                label="Adicionar ao limite"
                onPress={() => {
                  void act(async () => {
                    const cents = parseMoneyInput(extra);
                    if (BigInt(cents) === 0n) throw new Error('Informe um valor maior que zero.');
                    if (BigInt(cents) + BigInt(agent.budget_limit_cents!) > 9223372036854775807n)
                      throw new Error('O novo orçamento ultrapassa o limite permitido.');
                    await store.request({ op: 'raise_budget', agent_id: id, extra_cents: cents });
                    setExtra('');
                    setMessage('Orçamento atualizado.');
                  });
                }}
              />
            </>
          ) : null}
        </Surface>
      ) : null}
      {agent.spaces.map((space) => (
        <Surface key={space.space_id}>
          <Text style={{ color: colors.text, fontSize: 19, fontWeight: '600' }}>
            {space.space_title}
          </Text>
          {levels.map((level) => (
            <Pressable
              key={level.value}
              disabled={busy || !agent.persona.is_mine}
              accessibilityRole="radio"
              accessibilityState={{
                checked: space.level === level.value,
                disabled: busy || !agent.persona.is_mine,
              }}
              onPress={() => {
                if (space.level === level.value) return;
                void act(async () => {
                  if (
                    levels.findIndex((item) => item.value === level.value) >
                      levels.findIndex((item) => item.value === space.level) &&
                    !(await dialogs.confirm({
                      title: `Permitir ${level.title.toLowerCase()}?`,
                      message: `${agent.persona.name} · ${space.space_title}\n\n${level.detail} Ações que sempre pedem confirmação continuarão exigindo sua decisão.`,
                      confirmLabel: 'Atualizar autonomia',
                    }))
                  )
                    return;
                  await store.request({
                    op: 'set_trust',
                    agent_id: id,
                    space_id: space.space_id,
                    level: level.value,
                  });
                  setPreviewSpace(space.space_id);
                  setMessage('Autonomia atualizada nesta conversa.');
                });
              }}
              style={{ flexDirection: 'row', gap: 12, paddingVertical: 9 }}
            >
              <Text
                style={{
                  color: space.level === level.value ? colors.accent : colors.tertiary,
                  fontSize: 20,
                }}
              >
                {space.level === level.value ? '●' : '○'}
              </Text>
              <View style={{ flex: 1, gap: 5 }}>
                <Text style={{ color: colors.text, fontSize: 16, fontWeight: '600' }}>
                  {level.title}
                </Text>
                <Text style={{ color: colors.secondary, fontSize: 13, lineHeight: 19 }}>
                  {level.detail}
                </Text>
              </View>
            </Pressable>
          ))}
          <NativeButton
            secondary
            label="Ver permissões na prática"
            disabled={busy}
            onPress={() => setPreviewSpace(space.space_id)}
          />
        </Surface>
      ))}
      {selectedSpace ? (
        <Surface>
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 19, fontWeight: '600' }}
          >
            O que pode acontecer
          </Text>
          <Text style={{ color: colors.secondary }}>
            {agent.spaces.find((space) => space.space_id === selectedSpace)?.space_title}
          </Text>
          {previews.error ? (
            <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
              {previews.error}
            </Text>
          ) : !previews.data ? (
            <Loading />
          ) : (
            previews.data.map((preview) => (
              <View key={preview.action} style={{ gap: 5, paddingVertical: 8 }}>
                <Text
                  style={{
                    color: preview.red_line ? colors.danger : colors.text,
                    fontWeight: '600',
                  }}
                >
                  {preview.action} ·{' '}
                  {preview.kind === 'Act'
                    ? 'Executa'
                    : preview.kind === 'ActWithUndo'
                      ? 'Executa com desfazer'
                      : preview.kind === 'Request'
                        ? 'Pede aprovação'
                        : 'Bloqueado'}
                </Text>
                <Text style={{ color: colors.secondary, fontSize: 14 }}>{preview.example}</Text>
                <Text selectable style={{ color: colors.secondary, fontSize: 13, lineHeight: 20 }}>
                  {preview.explanation}
                </Text>
              </View>
            ))
          )}
        </Surface>
      ) : null}
      <Surface>
        <Text style={{ color: colors.text, fontSize: 19, fontWeight: '600' }}>
          Decisões permanentes
        </Text>
        {standing.error ? <Text style={{ color: colors.danger }}>{standing.error}</Text> : null}
        {standing.data && !standing.data.some((grant) => grant.agent.id === id) ? (
          <Text style={{ color: colors.secondary }}>
            Nenhuma decisão permanente para este agente.
          </Text>
        ) : null}
        {standing.data
          ?.filter((grant) => grant.agent.id === id)
          .map((grant) => (
            <View key={grant.grant_id} style={{ gap: 10, paddingVertical: 10 }}>
              <Text style={{ color: colors.text }}>
                {grant.allow ? 'Sempre permitir' : 'Sempre negar'} · {grant.action_label}
              </Text>
              <Text style={{ color: colors.secondary, fontSize: 13 }}>{grant.space_title}</Text>
              <NativeButton
                secondary
                label="Pedir de novo da próxima vez"
                disabled={busy}
                onPress={() => {
                  void act(() =>
                    store.request({ op: 'revoke_standing', grant_id: grant.grant_id }),
                  );
                }}
              />
            </View>
          ))}
      </Surface>
      {agent.persona.is_mine && mode === 'account' ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>Seu agente</Text>
          {status.error ? (
            <Text selectable style={{ color: colors.danger }}>
              {status.error}
            </Text>
          ) : null}
          <Text style={{ color: colors.secondary }}>
            {status.data === 'Active'
              ? 'Ativo'
              : status.data === 'Paused'
                ? 'Pausado'
                : status.data === 'Revoked'
                  ? 'Revogado'
                  : 'Consultando estado…'}
          </Text>
          {status.data === 'Active' || status.data === 'Paused' ? (
            <NativeButton
              secondary
              disabled={busy}
              label={status.data === 'Active' ? 'Pausar agente' : 'Retomar agente'}
              onPress={() => {
                void act(() =>
                  store.request({
                    op: 'control_agent',
                    agent_id: id,
                    action: status.data === 'Active' ? 'Pause' : 'Resume',
                  }),
                );
              }}
            />
          ) : null}
          <NativeButton
            secondary
            disabled={busy}
            label="Compartilhar agente"
            onPress={() => {
              void act(async () => {
                const invite = await store.driver.request({
                  op: 'create_agent_invite',
                  agent_id: id,
                });
                if (process.env.EXPO_OS === 'web') {
                  await Clipboard.setStringAsync(invite.link);
                  setMessage('Link de convite copiado.');
                } else await Share.share({ message: invite.link });
              });
            }}
          />
          <NativeButton
            secondary
            disabled={busy}
            label="Desativar link de convite"
            onPress={() => {
              void act(() => store.request({ op: 'revoke_agent_invite', agent_id: id }));
            }}
          />
          {status.data && status.data !== 'Revoked' ? (
            <NativeButton
              secondary
              disabled={busy}
              label="Revogar agente"
              onPress={() => {
                void dialogs
                  .confirm({
                    title: 'Revogar este agente?',
                    message: 'Esta ação encerra o acesso do agente e desativa seu convite.',
                    confirmLabel: 'Revogar agente',
                    destructive: true,
                  })
                  .then((accepted) => {
                    if (accepted)
                      void act(() =>
                        store.request({ op: 'control_agent', agent_id: id, action: 'Revoke' }),
                      );
                  });
              }}
            />
          ) : null}
        </Surface>
      ) : null}
      <Surface>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 19, fontWeight: '600' }}
        >
          Atividade recente
        </Text>
        <Text style={{ color: colors.secondary, fontSize: 12 }}>
          Lida do histórico assinado das conversas.
        </Text>
        {activity.error ? (
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
            {activity.error}
          </Text>
        ) : activity.data?.length === 0 ? (
          <Text style={{ color: colors.secondary }}>Nenhuma atividade ainda.</Text>
        ) : (
          activity.data?.map((entry, index) => (
            <Pressable
              key={`${entry.space_id}:${entry.at_ms}:${index}`}
              accessibilityRole="button"
              onPress={() =>
                router.push({ pathname: '/chat/[id]', params: { id: entry.space_id } })
              }
              style={{ gap: 6, paddingVertical: 10 }}
            >
              <Text style={{ color: colors.text, fontWeight: '600' }}>
                {entry.label}: {entry.detail}
              </Text>
              <Text style={{ color: colors.secondary, fontSize: 12 }}>
                {entry.space_title} · {new Date(Number(entry.at_ms)).toLocaleString('pt-BR')}
                {entry.cost_cents === null ? '' : ` · ${money(entry.cost_cents)}`}
              </Text>
            </Pressable>
          ))
        )}
      </Surface>
    </ScrollView>
  );
}
