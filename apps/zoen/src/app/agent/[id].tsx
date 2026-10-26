import { t, formatLocale } from '../../i18n/index.ts';
import { useState } from 'react';
import { router, useLocalSearchParams } from 'expo-router';
import { Pressable, Share, Text, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import { money } from '../../core/presentation';
import { agentTrustLevels, confirmAgentTrust } from '../../agents/trust-confirmation';
import { Avatar, EmptyState, Loading, NativeButton, Surface } from '../../components/primitives';
import { SettingsRows } from '../../components/settings-rows';
import { NavigationScrollView } from '../../components/navigation-scroll-view';
import { useTheme } from '../../theme';
import { useDialogs } from '../../components/dialogs';
import { useAgentControl } from '../../agents/use-agent-control';
import { AgentControlView } from '../../agents/agent-control-view';

export default function Agent() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { store, mode } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const levels = agentTrustLevels();
  const { data: agent, error, retry } = useCoreQuery({ op: 'agent_profile', agent_id: id });
  const standing = useCoreQuery({ op: 'standing_decisions' });
  const control = useAgentControl(id, mode === 'account' && !!agent?.persona.is_mine);
  const controlUnavailable = control.state.busy || control.state.pending !== null;
  const activity = useCoreQuery({ op: 'agent_activity', agent_id: id });
  const [previewSpace, setPreviewSpace] = useState<string | null>(null);
  const selectedSpace = previewSpace ?? agent?.spaces[0]?.space_id;
  const previews = useCoreQuery(
    selectedSpace ? { op: 'preview_decisions', agent_id: id, space_id: selectedSpace } : null,
  );
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
      void control.session.refresh();
      retry();
      standing.retry();
      activity.retry();
      previews.retry();
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : t('agents.thisCouldNotBeCompleted'));
    } finally {
      setBusy(false);
    }
  }
  if (!agent)
    return (
      <NavigationScrollView title={t(error ? 'agents.agentUnavailable' : 'common.loading')}>
        {error ? (
          <EmptyState title={t('agents.agentUnavailable')} detail={error}>
            <NativeButton label={t('agents.tryAgain')} onPress={retry} />
          </EmptyState>
        ) : (
          <Loading />
        )}
      </NavigationScrollView>
    );
  return (
    <NavigationScrollView
      title={agent.persona.name}
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
      <View style={{ alignItems: 'center', gap: 14, paddingVertical: 18 }}>
        <Avatar name={agent.persona.name} color={agent.persona.tint_hex} size={88} agent />
        <Text style={{ color: colors.text, fontSize: 29, fontWeight: '600' }}>
          {agent.persona.name}
        </Text>
        {agent.persona.bio ? (
          <Text selectable style={{ color: colors.secondary, lineHeight: 23, textAlign: 'center' }}>
            {agent.persona.bio}
          </Text>
        ) : null}
        <Text style={{ color: colors.secondary }}>
          {agent.persona.is_mine
            ? t('agents.youManageThisAgent')
            : t('agents.managedBy', { name: agent.persona.owner_name ?? t('agents.someoneElse') })}
        </Text>
        <NativeButton
          secondary
          label={
            agent.owner_notes_pending
              ? t('agents.chatPendingNotes', { count: agent.owner_notes_pending })
              : t('agents.chatWithAgent')
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
      {mode === 'account' && agent.persona.is_mine ? (
        <SettingsRows
          testID="agent-account-budget"
          rows={[
            {
              id: 'agent-account-budget-open',
              title: t('ownerPolicy.title'),
              icon: {
                ios: 'creditcard',
                android: 'account_balance_wallet',
                web: 'account_balance_wallet',
              },
              onPress: () => router.push({ pathname: '/account-budget', params: { agent: id } }),
            },
          ]}
        />
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
              aria-checked={space.level === level.value}
              onPress={() => {
                if (space.level === level.value) return;
                void act(async () => {
                  if (
                    !(await confirmAgentTrust(
                      dialogs,
                      agent.persona.name,
                      space.space_title,
                      space.level,
                      level.value,
                    ))
                  )
                    return;
                  await store.request({
                    op: 'set_trust',
                    agent_id: id,
                    space_id: space.space_id,
                    level: level.value,
                  });
                  setPreviewSpace(space.space_id);
                  setMessage(t('agents.autonomyUpdatedInThisChat'));
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
            label={t('agents.seePermissionsInPractice')}
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
            {t('agents.whatCanHappen')}
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
                    ? t('agents.performs')
                    : preview.kind === 'ActWithUndo'
                      ? t('agents.performsWithUndo')
                      : preview.kind === 'Request'
                        ? t('agents.asksForApproval')
                        : t('agents.blocked')}
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
          {t('agents.standingDecisions')}
        </Text>
        {standing.error ? <Text style={{ color: colors.danger }}>{standing.error}</Text> : null}
        {standing.data && !standing.data.some((grant) => grant.agent.id === id) ? (
          <Text style={{ color: colors.secondary }}>
            {t('agents.noStandingDecisionsForThisAgent')}
          </Text>
        ) : null}
        {standing.data
          ?.filter((grant) => grant.agent.id === id)
          .map((grant) => (
            <View key={grant.grant_id} style={{ gap: 10, paddingVertical: 10 }}>
              <Text style={{ color: colors.text }}>
                {grant.allow ? t('agents.alwaysAllow') : t('agents.alwaysDeny')} ·{' '}
                {grant.action_label}
              </Text>
              <Text style={{ color: colors.secondary, fontSize: 13 }}>{grant.space_title}</Text>
              <NativeButton
                secondary
                label={t('agents.askAgainNextTime')}
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
          <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
            {t('agents.yourAgent')}
          </Text>
          <AgentControlView state={control.state} session={control.session} disabled={busy} />
          {process.env.EXPO_OS === 'ios' ? (
            <NativeButton
              testID="agent-definition-open"
              secondary
              label={t('agentDefinition.title')}
              onPress={() => router.push({ pathname: '/agent-definition/[id]', params: { id } })}
            />
          ) : null}
          <NativeButton
            secondary
            disabled={busy}
            label={t('agents.shareAgent')}
            onPress={() => {
              void act(async () => {
                const invite = await store.driver.request({
                  op: 'create_agent_invite',
                  agent_id: id,
                });
                if (process.env.EXPO_OS === 'web') {
                  await Clipboard.setStringAsync(invite.link);
                  setMessage(t('agents.inviteLinkCopied'));
                } else await Share.share({ message: invite.link });
              });
            }}
          />
          <NativeButton
            secondary
            disabled={busy}
            label={t('agents.disableInviteLink')}
            onPress={() => {
              void act(() => store.request({ op: 'revoke_agent_invite', agent_id: id }));
            }}
          />
          {control.state.status && control.state.status !== 'Revoked' ? (
            <NativeButton
              secondary
              disabled={busy || controlUnavailable}
              label={t('agents.revokeAgent')}
              onPress={() => {
                void dialogs
                  .confirm({
                    title: t('agents.revokeThisAgent'),
                    message: t('agents.thisEndsTheAgentSAccessAndDisables'),
                    confirmLabel: t('agents.revokeAgent'),
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
          {t('agents.recentActivity')}
        </Text>
        <Text style={{ color: colors.secondary, fontSize: 12 }}>
          {t('agents.readFromTheSignedChatHistory')}
        </Text>
        {activity.error ? (
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
            {activity.error}
          </Text>
        ) : activity.data?.length === 0 ? (
          <Text style={{ color: colors.secondary }}>{t('agents.noActivityYet')}</Text>
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
                {entry.space_title} · {new Date(Number(entry.at_ms)).toLocaleString(formatLocale)}
                {entry.cost_cents === null ? '' : ` · ${money(entry.cost_cents)}`}
              </Text>
            </Pressable>
          ))
        )}
      </Surface>
    </NavigationScrollView>
  );
}
