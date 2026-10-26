import { useEffect, useRef, useState } from 'react';
import { Text, View } from 'react-native';
import { NativeButton, IconButton } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { confirmAgentTrust } from '../agents/trust-confirmation';
import { useClient } from '../core/provider';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { Persona } from '../core/generated/Persona';

/** Explicit consent in the same overlay keeps the document and human draft mounted.
 * Agent profile and set_trust are the existing conversation permission path. */
export function EditorAgentPermissions({
  agent,
  spaceId,
  enabled,
  allowed,
  onBack,
  onGranted,
}: {
  agent: Persona;
  enabled: boolean;
  spaceId: string;
  allowed: () => boolean;
  onBack: () => void;
  onGranted: () => void;
}) {
  const { store } = useClient(),
    dialogs = useDialogs(),
    { colors } = useTheme();
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null);
  const grant = async () => {
    if (busy || !live.current || !allowed()) return;
    setBusy(true);
    setError(null);
    try {
      const profile = await store.driver.request({ op: 'agent_profile', agent_id: agent.id });
      if (
        !live.current ||
        !allowed() ||
        profile.persona.id !== agent.id ||
        !profile.persona.is_mine
      )
        return;
      const space = profile.spaces.find((candidate) => candidate.space_id === spaceId);
      if (!space) throw Error(t('miniapps.agentUnavailable'));
      const confirmed = await confirmAgentTrust(
        dialogs,
        profile.persona.name,
        space.space_title,
        space.level,
        'Suggest',
        true,
      );
      if (!confirmed || !live.current || !allowed()) return;
      await store.request({
        op: 'set_trust',
        agent_id: agent.id,
        space_id: spaceId,
        level: 'Suggest',
      });
      if (live.current) onGranted();
    } catch (reason) {
      if (live.current && allowed())
        setError(reason instanceof Error ? reason.message : t('agents.thisCouldNotBeCompleted'));
    } finally {
      if (live.current) setBusy(false);
    }
  };
  return (
    <View style={{ padding: 20, gap: 20 }}>
      <IconButton
        name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
        label={t('common.back')}
        onPress={onBack}
      />
      <Text style={{ color: colors.text, fontSize: 22, fontWeight: '600' }}>{agent.name}</Text>
      <NativeButton
        testID="editor-agent-allow-suggest"
        disabled={busy || !enabled}
        label={t('miniapps.allowAgentSuggestions')}
        onPress={() => {
          void grant();
        }}
      />
      {error ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
    </View>
  );
}
