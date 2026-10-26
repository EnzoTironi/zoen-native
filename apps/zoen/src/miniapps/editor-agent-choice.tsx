import { Host, Picker } from '@expo/ui';
import { NativeButton, Loading } from '../components/primitives';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type {
  EditorAgentState,
  EditorAgentSession,
  EditorAgentTicket,
} from './editor-agent-session';

export function EditorAgentChoice({
  state,
  session,
  ticket,
  onPermissions,
  onRefresh,
}: {
  state: EditorAgentState;
  session: EditorAgentSession;
  ticket: EditorAgentTicket | null;
  onPermissions: (id: string) => void;
  onRefresh: () => void;
}) {
  const { scheme } = useTheme();
  const eligible = state.targets?.agents.filter((candidate) => candidate.eligible) ?? [];
  const held = state.uncertain ? state.target : null;
  const agents = held ? [held.agent] : eligible.map((candidate) => candidate.agent);
  return (
    <>
      {agents.length ? (
        <Host matchContents={{ vertical: true }} style={{ width: '100%' }} colorScheme={scheme}>
          <Picker<string>
            testID="editor-agent-choice"
            appearance="menu"
            selectedValue={state.target?.agent.id ?? ''}
            enabled={!state.busy && !state.uncertain && state.targetPhase === 'ready'}
            onValueChange={(id) => session.selectAgent(id, state.targets?.revision ?? '', ticket)}
          >
            {!state.target ? <Picker.Item value="" label={t('miniapps.chooseEditAgent')} /> : null}
            {agents.map((agent) => (
              <Picker.Item key={agent.id} value={agent.id} label={agent.name} />
            ))}
          </Picker>
        </Host>
      ) : null}
      {!state.uncertain && state.targetPhase === 'loading' ? <Loading /> : null}
      {!state.uncertain && state.targetPhase === 'failed' ? (
        <NativeButton
          testID="editor-agent-targets-retry"
          secondary
          label={t('chat.tryAgain2')}
          onPress={onRefresh}
        />
      ) : null}
      {!state.uncertain
        ? state.targets?.agents
            .filter((candidate) => candidate.permission_required)
            .map(({ agent }) => (
              <NativeButton
                key={agent.id}
                testID={`editor-agent-permissions-${agent.id}`}
                secondary
                disabled={state.busy}
                label={t('miniapps.agentPermissions', { name: agent.name })}
                onPress={() => onPermissions(agent.id)}
              />
            ))
        : null}
    </>
  );
}
