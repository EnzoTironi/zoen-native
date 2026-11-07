import { useState } from 'react';
import { router } from 'expo-router';
import { Keyboard, Text, View } from 'react-native';
import { Field, Loading, NativeButton } from '../components/primitives';
import { SettingsScreen } from '../components/settings-screen';
import { ProfileHeader } from '../profile/profile-header';
import { ProfileSection } from '../profile/profile-section';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { useManagedAgent } from './use-managed-agent';

export function CreateAgentScreen() {
  const { session, state, active, available, ticket, ownerKey, open } = useManagedAgent();
  const { presentation: p } = useTheme();
  const [input, setInput] = useState({ owner: ownerKey, value: '' });
  const name = input.owner === ownerKey ? input.value : '';
  const view = state.view,
    preview = view?.preview;
  const paragraph = { color: p.secondary, fontSize: 15, lineHeight: 21 };
  const create = () => {
    Keyboard.dismiss();
    void session.start(name, ticket);
  };
  return (
    <SettingsScreen
      title={t('managedAgent.create')}
      testID="create-agent-screen"
      automaticallyAdjustKeyboardInsets
    >
      {!available ? (
        <Text accessibilityRole="alert" style={paragraph}>
          {process.env.EXPO_OS === 'ios'
            ? t('managedAgent.accountRequired')
            : t('managedAgent.unavailable')}
        </Text>
      ) : !active || !state.ready ? (
        state.error ? null : (
          <Loading label={t('managedAgent.waiting')} />
        )
      ) : !view ? (
        <View style={{ gap: 20 }}>
          <Field
            label={t('managedAgent.name')}
            placeholder={t('managedAgent.namePlaceholder')}
            testID="managed-agent-name"
            value={name}
            onChangeText={(value) => setInput({ owner: ownerKey, value })}
            maxLength={64}
            autoFocus
            editable={!state.busy}
            returnKeyType="done"
            onSubmitEditing={create}
          />
          <NativeButton
            label={t('managedAgent.create')}
            testID="managed-agent-create"
            disabled={state.busy || !name.trim()}
            onPress={create}
          />
        </View>
      ) : (
        <>
          <ProfileHeader name={view.name} agent />
          {view.state === 'review' && preview ? (
            <>
              <ProfileSection title={t('agentEnrollment.about')}>
                <Text
                  selectable
                  style={{ color: p.text, padding: 16, fontSize: 17, lineHeight: 23 }}
                >
                  {preview.persona}
                </Text>
              </ProfileSection>
              {preview.rules.length ? (
                <ProfileSection title={t('agentEnrollment.rules')}>
                  {preview.rules.map((rule, index) => (
                    <Text
                      key={index}
                      selectable
                      style={{ color: p.text, padding: 16, fontSize: 17, lineHeight: 23 }}
                    >
                      {rule}
                    </Text>
                  ))}
                </ProfileSection>
              ) : null}
              <Text style={paragraph}>{t('managedAgent.scope')}</Text>
              <NativeButton
                testID="managed-agent-confirm"
                label={t('managedAgent.confirm')}
                disabled={state.busy || !!state.error}
                onPress={() => {
                  void session.approve(ticket, view);
                }}
              />
            </>
          ) : view.state === 'registered' ? (
            <View style={{ gap: 16 }}>
              <Text
                accessibilityRole="header"
                style={{ color: p.text, fontSize: 20, fontWeight: '600', textAlign: 'center' }}
              >
                {t('managedAgent.ready')}
              </Text>
              <NativeButton
                label={t('managedAgent.budget')}
                testID="managed-agent-budget"
                disabled={state.busy}
                onPress={() => {
                  void open('budget');
                }}
              />
              <NativeButton
                label={t('managedAgent.chat')}
                testID="managed-agent-chat"
                disabled={state.busy}
                onPress={() => {
                  void open('chat');
                }}
              />
            </View>
          ) : ['expired', 'rejected', 'cancelled'].includes(view.state) ? (
            <View style={{ gap: 16 }}>
              <Text accessibilityRole="alert" style={paragraph}>
                {t(
                  view.state === 'cancelled'
                    ? 'managedAgent.cancelled'
                    : view.state === 'expired'
                      ? 'managedAgent.expired'
                      : 'managedAgent.rejected',
                )}
              </Text>
              <NativeButton
                label={t(view.state === 'cancelled' ? 'managedAgent.again' : 'managedAgent.retry')}
                disabled={state.busy}
                onPress={() => {
                  void session.restart(ticket, view.operation_id);
                }}
              />
            </View>
          ) : (
            <Loading
              label={t(
                view.state === 'cancelling'
                  ? 'managedAgent.cancelling'
                  : view.state === 'installing'
                    ? 'managedAgent.installing'
                    : 'managedAgent.waiting',
              )}
            />
          )}
        </>
      )}
      {active && state.ready && view && ['queued', 'review'].includes(view.state) ? (
        <NativeButton
          label={t('managedAgent.cancel')}
          secondary
          testID="managed-agent-cancel"
          disabled={state.busy}
          onPress={() => {
            void session.cancel(ticket, view.operation_id);
          }}
        />
      ) : null}
      {active && state.error ? (
        <View style={{ gap: 16 }}>
          <Text accessibilityRole="alert" style={{ ...paragraph, color: p.destructive }}>
            {t('managedAgent.failed')}
          </Text>
          <NativeButton
            label={t('managedAgent.retry')}
            secondary
            disabled={state.busy}
            onPress={() => {
              void session.refresh();
            }}
          />
        </View>
      ) : null}
      <NativeButton label={t('managedAgent.close')} secondary onPress={() => router.back()} />
    </SettingsScreen>
  );
}
