import { useRef, useState } from 'react';
import { router, useLocalSearchParams, usePreventRemove } from 'expo-router';
import { useAgentDefinition } from '../../agent-definition/use-agent-definition';
import { DefinitionView } from '../../agent-definition/definition-view';
import { useDialogs } from '../../components/dialogs';
import { t } from '../../i18n';

export default function AgentDefinition() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { session, state } = useAgentDefinition(id);
  const dialogs = useDialogs();
  const [leaving, setLeaving] = useState(false);
  const dialogPending = useRef(false);
  const dirty = session.dirty() && ['editing', 'previewing', 'review'].includes(state.phase);
  usePreventRemove(!leaving && dirty, ({ repeat }) => {
    const ticket = session.ticket();
    if (!ticket || dialogPending.current) return;
    dialogPending.current = true;
    void dialogs
      .confirm({
        title: t('agentDefinition.discard'),
        message: t('agentDefinition.discardDetail'),
        confirmLabel: t('agentDefinition.discardConfirm'),
        destructive: true,
      })
      .then((accepted) => {
        if (accepted && session.accepts(ticket)) {
          session.retire();
          setLeaving(true);
          repeat();
        }
      })
      .finally(() => {
        dialogPending.current = false;
      });
  });
  const observed = session.confirmation();
  const rendered = session.ticket();
  const active = () => rendered && session.acceptsSession(rendered);
  return (
    <DefinitionView
      state={state}
      dirty={dirty}
      onPersona={(value) => {
        if (active()) session.setPersona(value);
      }}
      onRule={(key, value) => {
        if (active()) session.setRule(key, value);
      }}
      onAddRule={() => {
        if (active()) session.addRule();
      }}
      onRemoveRule={(key) => {
        if (active()) session.removeRule(key);
      }}
      onPreview={() => {
        if (active()) void session.preview();
      }}
      onApply={() => {
        if (observed) void session.apply(observed);
      }}
      onEdit={() => {
        if (rendered && session.accepts(rendered)) session.edit(state.persona, state.rules);
      }}
      onReconcile={() => {
        if (active()) void session.reconcile();
      }}
      onDismissAttempt={() => {
        const observed = active() ? session.dismissal() : null;
        if (!observed || dialogPending.current) return;
        dialogPending.current = true;
        void dialogs
          .confirm({
            title: t('agentDefinition.another'),
            message: t('agentDefinition.anotherDetail'),
            confirmLabel: t('agentDefinition.another'),
          })
          .then((accepted) => {
            if (accepted) void session.dismiss(observed);
          })
          .finally(() => {
            dialogPending.current = false;
          });
      }}
      onReload={() => {
        const ticket = active() ? session.ticket() : null;
        if (!ticket || dialogPending.current) return;
        dialogPending.current = true;
        void dialogs
          .confirm({
            title: t('agentDefinition.discard'),
            message: t('agentDefinition.discardDetail'),
            confirmLabel: t('agentDefinition.reload'),
            destructive: true,
          })
          .then((accepted) => {
            if (accepted) void session.reload(ticket);
          })
          .finally(() => {
            dialogPending.current = false;
          });
      }}
      onRetry={() => {
        if (active()) void session.load();
      }}
      onDone={() => {
        if (active()) router.back();
      }}
    />
  );
}
