import { useCallback, useRef, useState } from 'react';
import { AppState, ScrollView, Text } from 'react-native';
import { Stack, useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { openAccountBudget, ownedBudgetAgents } from '../owner-policy/account-port';
import type { PolicyTarget } from '../owner-policy/session';
import { OwnerPolicyControllerScreen } from './owner-policy-screen';
import { SettingsRows } from '../components/settings-rows';
import { EmptyState } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';

/** Selection establishes a private control context, never a group post. */
export function AccountBudgetScreen({ agentId }: { agentId?: string }) {
  const client = useClient();
  const { store } = client;
  const { colors } = useTheme();
  const key = accountScopeKey(accountScope(store, client));
  const agents = ownedBudgetAgents(store, client).filter(
    (agent) => !agentId || agent.persona.id === agentId,
  );
  const [selection, setSelection] = useState<{ owner: string; target: PolicyTarget } | null>(null);
  const [renderedLease, setRenderedLease] = useState(0);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(false);
  const lease = useRef(0),
    active = useRef(false),
    inFlight = useRef(false);
  const automaticAttempt = useRef('');
  const ownedTarget =
    agentId && agents.some(({ persona }) => persona.id === agentId) ? agentId : null;
  const select = useCallback(
    (agent: string, observation: number) => {
      const owner = accountScope(store);
      if (
        !owner ||
        !active.current ||
        observation !== lease.current ||
        AppState.currentState !== 'active' ||
        inFlight.current ||
        accountScopeKey(owner) !== key
      )
        return;
      const generation = lease.current;
      inFlight.current = true;
      setBusy(true);
      setError(false);
      void openAccountBudget(store, owner, agent)
        .then(
          (target) => {
            if (
              active.current &&
              generation === lease.current &&
              accountScopeKey(accountScope(store)) === key
            )
              setSelection({ owner: key, target });
          },
          () => {
            if (active.current && generation === lease.current) setError(true);
          },
        )
        .finally(() => {
          if (active.current && generation === lease.current) {
            inFlight.current = false;
            setBusy(false);
          }
        });
    },
    [store, key],
  );
  useFocusEffect(
    useCallback(() => {
      let focused = true;
      const retire = () => {
        active.current = false;
        lease.current++;
        inFlight.current = false;
      };
      const activate = () => {
        if (!focused) return;
        retire();
        active.current =
          AppState.currentState === 'active' && accountScopeKey(accountScope(store)) === key;
        setRenderedLease(lease.current);
        setBusy(false);
        const attempt = JSON.stringify([key, ownedTarget]);
        // The specific Budget navigation is already an explicit selection. Open
        // its private context once; this never confirms or grants a policy.
        if (active.current && ownedTarget && automaticAttempt.current !== attempt) {
          automaticAttempt.current = attempt;
          select(ownedTarget, lease.current);
        }
      };
      const preflight = store.registerSessionPreflight(() => {
        if (focused) retire();
      });
      const changes = store.subscribe(() => {
        if (focused && accountScopeKey(accountScope(store)) !== key) retire();
      });
      const app = AppState.addEventListener('change', (state) => {
        if (!focused) return;
        if (state !== 'active') retire();
        else activate();
      });
      activate();
      return () => {
        focused = false;
        retire();
        preflight();
        changes();
        app.remove();
      };
    }, [store, key, ownedTarget, select]),
  );
  if (!key) return <EmptyState title={t('ownerPolicy.unavailable')} />;
  if (selection?.owner === key) return <OwnerPolicyControllerScreen target={selection.target} />;
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ padding: 24, gap: 20 }}
    >
      <Stack.Screen options={{ title: t('ownerPolicy.title') }} />
      {agents.length ? (
        <SettingsRows
          testID="account-budget-agents"
          rows={agents.map(({ persona }) => ({
            id: `account-budget-agent-${persona.id}`,
            title: persona.name,
            icon: { ios: 'person.crop.circle', android: 'smart_toy', web: 'smart_toy' },
            disabled: busy,
            onPress: () => select(persona.id, renderedLease),
          }))}
        />
      ) : (
        <EmptyState title={t('ownerPolicy.noOwnedAgents')} />
      )}
      {error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
          {t('ownerPolicy.operationFailed')}
        </Text>
      ) : null}
    </ScrollView>
  );
}
