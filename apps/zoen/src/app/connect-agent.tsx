import { useEffect } from 'react';
import { router } from 'expo-router';
import { ConnectAgentView } from '../agent-enrollment/connect-agent-view';
import { useAgentEnrollment } from '../agent-enrollment/use-agent-enrollment';
import { useDialogs } from '../components/dialogs';
import { t } from '../i18n';

export default function ConnectAgent() {
  const flow = useAgentEnrollment();
  const dialogs = useDialogs();
  const confirmation = flow.session.confirmation();
  useEffect(() => {
    if (flow.state.phase !== 'review' || !flow.state.review) return;
    const delay = flow.state.review.expires - BigInt(Date.now());
    const timer = setTimeout(() => flow.session.expire(), Number(delay > 0n ? delay : 0n));
    return () => clearTimeout(timer);
  }, [flow.session, flow.state.phase, flow.state.review]);
  return (
    <ConnectAgentView
      state={flow.state}
      availability={flow.availability}
      scannerAvailable={flow.scannerAvailable}
      scannerError={flow.scannerError}
      onScan={() => {
        void flow.scan();
      }}
      onCode={flow.setCode}
      onCheck={() => {
        void flow.session.check();
      }}
      onConfirm={() => {
        if (confirmation) void flow.session.approve(confirmation);
      }}
      onNewCode={() => {
        flow.setCode('');
      }}
      onReconcile={() => {
        void flow.session.reconcile();
      }}
      onRetryRestore={flow.retryRestore}
      onDismiss={() => {
        flow.dismiss();
        router.back();
      }}
      onProfile={() => {
        // No navigation from an old render, unverified QR profile or late response.
        if (flow.state !== flow.session.getSnapshot() || flow.state.phase !== 'registered') return;
        router.replace({ pathname: '/agent/[id]', params: { id: flow.state.review!.agent } });
      }}
      onStartAnother={() => {
        const observed = flow.session.restartConfirmation();
        if (!observed) return;
        void dialogs
          .confirm({
            title: t('agentEnrollment.another'),
            message: t('agentEnrollment.anotherExplanation'),
            confirmLabel: t('agentEnrollment.another'),
          })
          .then((confirmed) => {
            if (confirmed) void flow.session.startAnother(observed);
          });
      }}
    />
  );
}
