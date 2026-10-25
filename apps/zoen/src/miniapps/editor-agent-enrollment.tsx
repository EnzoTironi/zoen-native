import { useEffect, useState } from 'react';
import { ScrollView, Text, View } from 'react-native';
import { ConnectAgentView } from '../agent-enrollment/connect-agent-view';
import { useAgentEnrollment } from '../agent-enrollment/use-agent-enrollment';
import type { EnrollmentRestart } from '../agent-enrollment/session';
import { IconButton, NativeButton } from '../components/primitives';
import { useTheme } from '../theme';
import { t } from '../i18n';

/** Retained in the editor's one sheet. Every input belongs to its visible ticket. */
export function EditorAgentEnrollment({
  spaceId,
  active,
  allowed,
  onBack,
  onClose,
  onContinue,
}: {
  spaceId: string;
  active: boolean;
  allowed: () => boolean;
  onBack: () => void;
  onClose: () => void;
  onContinue: () => void;
}) {
  const flow = useAgentEnrollment(spaceId);
  const { colors } = useTheme();
  const [restart, setRestart] = useState<EnrollmentRestart | null>(null);
  const current = () => active && allowed() && flow.state === flow.session.getSnapshot();
  const run = (action: () => void) => {
    if (current()) action();
  };
  const confirmation = flow.session.confirmation();
  useEffect(() => {
    if (!active || flow.state.phase !== 'review' || !flow.state.review) return;
    const delay = flow.state.review.expires - BigInt(Date.now());
    const timer = setTimeout(() => flow.session.expire(), Number(delay > 0n ? delay : 0n));
    return () => clearTimeout(timer);
  }, [active, flow.session, flow.state.phase, flow.state.review]);
  return (
    <ScrollView
      keyboardShouldPersistTaps="always"
      automaticallyAdjustKeyboardInsets
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ padding: 20, gap: 16, paddingBottom: 32 }}
    >
      <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
        <IconButton
          name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
          label={t('common.back')}
          onPress={() =>
            run(() => {
              setRestart(null);
              onBack();
            })
          }
        />
        <IconButton
          name={{ ios: 'xmark', android: 'close', web: 'close' }}
          label={t('common.close')}
          onPress={() => run(onClose)}
        />
      </View>
      {restart ? (
        <View style={{ gap: 16 }}>
          <Text style={{ color: colors.text }}>{t('agentEnrollment.anotherExplanation')}</Text>
          <NativeButton
            label={t('agentEnrollment.another')}
            onPress={() =>
              run(() => {
                const observed = restart;
                setRestart(null);
                void flow.session.startAnother(observed);
              })
            }
          />
          <NativeButton
            secondary
            label={t('common.cancel')}
            onPress={() => run(() => setRestart(null))}
          />
        </View>
      ) : (
        <ConnectAgentView
          embedded
          profileLabel={t('common.continue')}
          state={flow.state}
          availability={flow.availability}
          scannerAvailable={flow.scannerAvailable}
          scannerError={flow.scannerError}
          onScan={() =>
            run(() => {
              void flow.scan(current);
            })
          }
          onCode={(code) => run(() => flow.setCode(code))}
          onCheck={() =>
            run(() => {
              void flow.session.check();
            })
          }
          onConfirm={() =>
            run(() => {
              if (confirmation) void flow.session.approve(confirmation);
            })
          }
          onNewCode={() => run(() => flow.setCode(''))}
          onReconcile={() =>
            run(() => {
              void flow.session.reconcile();
            })
          }
          onRetryRestore={() => run(flow.retryRestore)}
          onDismiss={() => run(flow.state.phase === 'registered' ? onContinue : onBack)}
          onProfile={() =>
            run(() => {
              if (flow.state.phase === 'registered') onContinue();
            })
          }
          onStartAnother={() => run(() => setRestart(flow.session.restartConfirmation()))}
        />
      )}
    </ScrollView>
  );
}
