import { useState } from 'react';
import { Keyboard, Text, View } from 'react-native';
import { Field, Icon, Loading, NativeButton } from '../components/primitives';
import { SettingsRows } from '../components/settings-rows';
import { SettingsScreen } from '../components/settings-screen';
import { ProfileHeader } from '../profile/profile-header';
import { ProfileSection } from '../profile/profile-section';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { EnrollmentState } from './session';

export type ConnectAgentViewProps = {
  state: EnrollmentState;
  availability: 'ready' | 'loading' | 'restore-error' | 'account-required' | 'unsupported';
  scannerAvailable: boolean;
  scannerError: 'denied' | 'unavailable' | null;
  onScan: () => void;
  onCode: (code: string) => void;
  onCheck: () => void;
  onConfirm: () => void;
  onNewCode: () => void;
  onReconcile: () => void;
  onRetryRestore: () => void;
  onDismiss: () => void;
  onProfile: () => void;
  onStartAnother: () => void;
};

export function ConnectAgentView(props: ConnectAgentViewProps) {
  const { state } = props;
  const { presentation: p } = useTheme();
  const [enterCode, setEnterCode] = useState(false);
  const review = state.review;
  const paragraph = { color: p.secondary, fontSize: 15, lineHeight: 21 };
  const unavailable = props.availability !== 'ready';
  const checkCode = () => {
    if (unavailable || state.phase !== 'entry' || !state.code.trim()) return;
    Keyboard.dismiss();
    props.onCheck();
  };
  return (
    <SettingsScreen
      title={t('agentEnrollment.connect')}
      testID="agent-enrollment-screen"
      keyboardShouldPersistTaps="always"
      automaticallyAdjustKeyboardInsets
    >
      {unavailable ? (
        props.availability === 'loading' ? (
          <Loading label={t('agentEnrollment.checking')} />
        ) : (
          <View style={{ gap: 16 }}>
            <Text selectable accessibilityRole="alert" style={paragraph}>
              {props.availability === 'restore-error'
                ? t('agentEnrollment.restoreFailed')
                : props.availability === 'unsupported'
                  ? t('agentEnrollment.unavailable')
                  : t('agentEnrollment.accountRequired')}
            </Text>
            {props.availability === 'restore-error' ? (
              <NativeButton label={t('agentEnrollment.retry')} onPress={props.onRetryRestore} />
            ) : null}
          </View>
        )
      ) : review ? (
        <>
          <ProfileHeader name={review.name} subtitle={`@${review.handle}`} agent />
          {state.phase === 'review' || state.phase === 'submitting' ? (
            <>
              <ProfileSection title={t('agentEnrollment.about')}>
                <Text
                  selectable
                  style={{ padding: 16, color: p.text, fontSize: 17, lineHeight: 23 }}
                >
                  {review.persona}
                </Text>
              </ProfileSection>
              {review.rules.length ? (
                <ProfileSection title={t('agentEnrollment.rules')}>
                  {review.rules.map((rule, index) => (
                    <Text
                      key={index}
                      selectable
                      style={{ padding: 16, color: p.text, fontSize: 17, lineHeight: 23 }}
                    >
                      {rule}
                    </Text>
                  ))}
                </ProfileSection>
              ) : null}
              <ProfileSection footer={t('agentEnrollment.compareExplanation')}>
                <View style={{ gap: 12, alignItems: 'center', padding: 20 }}>
                  <Text style={{ ...p.typography.footnote, color: p.secondary }}>
                    {t('agentEnrollment.compare')}
                  </Text>
                  <Text
                    testID="agent-enrollment-pairing-code"
                    selectable
                    accessibilityLabel={`${t('agentEnrollment.compare')}: ${review.pairing}`}
                    style={{
                      color: p.text,
                      fontSize: 32,
                      fontWeight: '600',
                      fontVariant: ['tabular-nums'],
                      textAlign: 'center',
                      letterSpacing: 4,
                    }}
                  >
                    {review.pairing}
                  </Text>
                </View>
              </ProfileSection>
              <Text selectable style={paragraph}>
                {t('agentEnrollment.scope')}
              </Text>
              {state.error === 'expired' ? (
                <Text
                  selectable
                  accessibilityRole="alert"
                  style={{ ...paragraph, color: p.destructive }}
                >
                  {t('agentEnrollment.expired')}
                </Text>
              ) : null}
              <NativeButton
                testID="agent-enrollment-confirm"
                label={
                  state.phase === 'submitting'
                    ? t('agentEnrollment.sending')
                    : t('agentEnrollment.confirm')
                }
                disabled={state.phase === 'submitting' || state.error === 'expired'}
                onPress={props.onConfirm}
              />
              {state.error === 'expired' ? (
                <NativeButton
                  testID="agent-enrollment-new-code"
                  secondary
                  label={t('agentEnrollment.newCode')}
                  onPress={props.onNewCode}
                />
              ) : null}
              <NativeButton
                secondary
                label={
                  state.phase === 'submitting'
                    ? t('agentEnrollment.done')
                    : t('agentEnrollment.cancel')
                }
                onPress={props.onDismiss}
              />
              {state.phase === 'submitting' ? (
                <Text selectable style={paragraph}>
                  {t('agentEnrollment.deliveryNotice')}
                </Text>
              ) : null}
            </>
          ) : (
            <View testID="agent-enrollment-delivery-state" style={{ gap: 18 }}>
              <Text
                accessibilityRole="header"
                style={{ color: p.text, fontSize: 20, fontWeight: '600' }}
              >
                {state.phase === 'registered'
                  ? t('agentEnrollment.connected')
                  : state.phase === 'revoked'
                    ? t('agentEnrollment.revoked')
                    : t('agentEnrollment.waiting')}
              </Text>
              <Text selectable style={paragraph}>
                {state.phase === 'registered'
                  ? t('agentEnrollment.connectedExplanation')
                  : state.phase === 'revoked'
                    ? t('agentEnrollment.revokedExplanation')
                    : state.outcome === 'Sent'
                      ? t('agentEnrollment.sent')
                      : t('agentEnrollment.unknown')}
              </Text>
              {state.error === 'status' || state.error === 'dismiss' ? (
                <Text
                  selectable
                  accessibilityRole="alert"
                  style={{ ...paragraph, color: p.destructive }}
                >
                  {state.error === 'dismiss'
                    ? t('agentEnrollment.anotherFailed')
                    : t('agentEnrollment.statusFailed')}
                </Text>
              ) : null}
              {state.phase !== 'revoked' ? (
                <NativeButton
                  testID={
                    state.phase === 'registered'
                      ? 'agent-enrollment-profile'
                      : 'agent-enrollment-status'
                  }
                  label={
                    state.phase === 'registered'
                      ? t('agentEnrollment.profile')
                      : state.reconciling
                        ? t('agentEnrollment.statusChecking')
                        : t('agentEnrollment.status')
                  }
                  disabled={state.reconciling || state.dismissing}
                  onPress={state.phase === 'registered' ? props.onProfile : props.onReconcile}
                />
              ) : null}
              <NativeButton secondary label={t('agentEnrollment.done')} onPress={props.onDismiss} />
              <NativeButton
                testID="agent-enrollment-another"
                secondary
                label={t('agentEnrollment.another')}
                disabled={!state.receiptKnown || state.reconciling || state.dismissing}
                onPress={props.onStartAnother}
              />
              {state.phase === 'waiting' ? (
                <Text selectable style={paragraph}>
                  {t('agentEnrollment.deliveryNotice')}
                </Text>
              ) : null}
            </View>
          )}
        </>
      ) : (
        <>
          <View style={{ gap: 18, alignItems: 'center', paddingTop: 16 }}>
            <Icon
              name={{
                ios: 'qrcode.viewfinder',
                android: 'qr_code_scanner',
                web: 'qr_code_scanner',
              }}
              size={64}
              color={p.accent}
            />
            <Text
              accessibilityRole="header"
              style={{ color: p.text, fontSize: 23, fontWeight: '600', textAlign: 'center' }}
            >
              {t('agentEnrollment.introduction')}
            </Text>
            <Text style={{ ...paragraph, textAlign: 'center' }}>
              {t('agentEnrollment.explanation')}
            </Text>
          </View>
          {state.phase === 'checking' ? (
            <Loading label={t('agentEnrollment.checking')} />
          ) : (
            <SettingsRows
              testID="agent-enrollment-input-options"
              rows={[
                ...(props.scannerAvailable
                  ? [
                      {
                        id: 'agent-enrollment-scan',
                        title: t('agentEnrollment.scan'),
                        icon: {
                          ios: 'qrcode.viewfinder' as const,
                          android: 'qr_code_scanner' as const,
                          web: 'qr_code_scanner' as const,
                        },
                        onPress: props.onScan,
                      },
                    ]
                  : []),
                {
                  id: 'agent-enrollment-paste',
                  title: t('agentEnrollment.paste'),
                  icon: {
                    ios: 'text.cursor' as const,
                    android: 'edit' as const,
                    web: 'edit' as const,
                  },
                  onPress: () => setEnterCode(true),
                },
              ]}
            />
          )}
          {enterCode || state.code ? (
            <View
              testID="agent-enrollment-code-row"
              style={{ flexDirection: 'row', flexWrap: 'wrap', alignItems: 'flex-end', gap: 12 }}
            >
              <View style={{ flexGrow: 1, flexBasis: 200, minWidth: 160, maxWidth: '100%' }}>
                <Field
                  testID="agent-enrollment-code-input"
                  label={t('agentEnrollment.code')}
                  value={state.code}
                  onChangeText={props.onCode}
                  autoCapitalize="none"
                  autoCorrect={false}
                  spellCheck={false}
                  autoComplete="off"
                  textContentType="none"
                  importantForAutofill="no"
                  secureTextEntry
                  multiline={false}
                  returnKeyType="go"
                  onSubmitEditing={checkCode}
                  editable={state.phase !== 'checking'}
                />
              </View>
              <View style={{ flexShrink: 1, maxWidth: '100%', paddingBottom: 5 }}>
                <NativeButton
                  testID="agent-enrollment-check"
                  label={t('agentEnrollment.checkCode')}
                  disabled={!state.code.trim() || state.phase === 'checking'}
                  onPress={checkCode}
                />
              </View>
            </View>
          ) : null}
          {props.scannerError || state.error === 'preview' ? (
            <Text
              selectable
              accessibilityRole="alert"
              style={{ ...paragraph, color: p.destructive }}
            >
              {state.error === 'preview'
                ? t('agentEnrollment.previewFailed')
                : props.scannerError === 'denied'
                  ? t('agentEnrollment.cameraDenied')
                  : t('agentEnrollment.scannerFailed')}
            </Text>
          ) : null}
          <NativeButton secondary label={t('agentEnrollment.cancel')} onPress={props.onDismiss} />
        </>
      )}
    </SettingsScreen>
  );
}
