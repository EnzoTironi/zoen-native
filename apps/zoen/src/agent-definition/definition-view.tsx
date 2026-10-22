import { Keyboard, Text, View } from 'react-native';
import { Field, Loading, NativeButton } from '../components/primitives';
import { SettingsScreen } from '../components/settings-screen';
import { ProfileSection } from '../profile/profile-section';
import { useTheme } from '../theme';
import { t } from '../i18n';
import type { AgentDefinitionDto } from '../core/generated/AgentDefinitionDto';
import type { DefinitionState } from './session';

export type DefinitionViewProps = {
  state: DefinitionState;
  dirty: boolean;
  onPersona(value: string): void;
  onRule(key: number, value: string): void;
  onAddRule(): void;
  onRemoveRule(key: number): void;
  onPreview(): void;
  onApply(): void;
  onEdit(): void;
  onReconcile(): void;
  onDismissAttempt(): void;
  onReload(): void;
  onRetry(): void;
  onDone(): void;
};
function Behavior({ value, title }: { value: AgentDefinitionDto; title: string }) {
  const { presentation: p } = useTheme();
  return (
    <ProfileSection title={title}>
      <View style={{ padding: 16, gap: 16 }}>
        <Text selectable style={{ color: p.text, fontSize: 17, lineHeight: 24 }}>
          {value.persona}
        </Text>
        {value.rules.length ? (
          value.rules.map((rule, index) => (
            <Text key={index} selectable style={{ color: p.text, fontSize: 17, lineHeight: 24 }}>
              {index + 1}. {rule}
            </Text>
          ))
        ) : (
          <Text style={{ color: p.secondary }}>{t('agentDefinition.noRules')}</Text>
        )}
      </View>
    </ProfileSection>
  );
}
export function DefinitionView(props: DefinitionViewProps) {
  const { state } = props;
  const { presentation: p } = useTheme();
  const paragraph = { color: p.secondary, fontSize: 15, lineHeight: 22 };
  const revoked = state.current?.status === 'Revoked';
  const checking = state.phase === 'previewing';
  const held = state.phase === 'held';
  const submitting = state.phase === 'submitting';
  const outcome = state.receipt?.outcome ?? 'Unknown';
  return (
    <SettingsScreen
      title={t('agentDefinition.title')}
      testID="agent-definition-screen"
      keyboardShouldPersistTaps="always"
      automaticallyAdjustKeyboardInsets
    >
      {state.phase === 'loading' ? (
        <Loading label={t('agentDefinition.loading')} />
      ) : state.phase === 'unavailable' ? (
        <Text accessibilityRole="alert" style={paragraph}>
          {t('agentDefinition.unavailable')}
        </Text>
      ) : state.phase === 'error' ? (
        <>
          <Text accessibilityRole="alert" style={paragraph}>
            {t('agentDefinition.loadError')}
          </Text>
          <NativeButton
            testID="agent-definition-retry"
            label={t('agentDefinition.retry')}
            onPress={props.onRetry}
          />
        </>
      ) : (
        <>
          {state.phase === 'editing' || checking ? (
            <>
              <Text style={paragraph}>{t('agentDefinition.description')}</Text>
              {revoked ? (
                <Text accessibilityRole="alert" style={paragraph}>
                  {t('agentDefinition.revoked')}
                </Text>
              ) : null}
              <Field
                testID="agent-definition-persona"
                label={t('agentDefinition.persona')}
                value={state.persona}
                onChangeText={props.onPersona}
                multiline
                editable={!revoked}
              />
              <ProfileSection title={t('agentDefinition.rules')}>
                <View style={{ padding: 16, gap: 20 }}>
                  {state.rules.length ? (
                    state.rules.map((rule, index) => {
                      const key = state.ruleKeys[index]!;
                      return (
                        <View key={key} style={{ gap: 8 }}>
                          <Field
                            testID={`agent-definition-rule-${key}`}
                            label={t('agentDefinition.rule', { number: index + 1 })}
                            value={rule}
                            onChangeText={(value) => props.onRule(key, value)}
                            editable={!revoked}
                          />
                          {!revoked ? (
                            <NativeButton
                              secondary
                              label={t('agentDefinition.removeRule', { number: index + 1 })}
                              onPress={() => props.onRemoveRule(key)}
                            />
                          ) : null}
                        </View>
                      );
                    })
                  ) : (
                    <Text style={paragraph}>{t('agentDefinition.noRules')}</Text>
                  )}
                  {!revoked ? (
                    <NativeButton
                      testID="agent-definition-add-rule"
                      secondary
                      disabled={state.rules.length >= 8}
                      label={t('agentDefinition.addRule')}
                      onPress={props.onAddRule}
                    />
                  ) : null}
                </View>
              </ProfileSection>
              {state.error === 'preview' ? (
                <>
                  <Text accessibilityRole="alert" style={{ ...paragraph, color: p.destructive }}>
                    {t('agentDefinition.previewError')}
                  </Text>
                  <NativeButton
                    secondary
                    label={t('agentDefinition.reload')}
                    onPress={props.onReload}
                  />
                </>
              ) : null}
              {!revoked ? (
                <NativeButton
                  testID="agent-definition-review"
                  label={checking ? t('agentDefinition.reviewing') : t('agentDefinition.review')}
                  disabled={!props.dirty || checking}
                  onPress={() => {
                    Keyboard.dismiss();
                    props.onPreview();
                  }}
                />
              ) : null}
            </>
          ) : state.review ? (
            <>
              {held || submitting ? (
                <Text
                  testID="agent-definition-outcome"
                  accessibilityRole="header"
                  style={{ color: p.text, fontSize: 23, fontWeight: '600' }}
                >
                  {submitting
                    ? t('agentDefinition.applying')
                    : outcome === 'Confirmed'
                      ? t('agentDefinition.saved')
                      : outcome === 'Conflict'
                        ? t('agentDefinition.conflict')
                        : t('agentDefinition.unknown')}
                </Text>
              ) : null}
              {held && outcome !== 'Confirmed' ? (
                <Text style={paragraph}>
                  {outcome === 'Conflict'
                    ? t('agentDefinition.conflictDetail')
                    : t('agentDefinition.unknownDetail')}
                </Text>
              ) : null}
              <Behavior value={state.review.current} title={t('agentDefinition.before')} />
              <Behavior value={state.review.proposed} title={t('agentDefinition.after')} />
              {held && state.current ? (
                <Behavior value={state.current} title={t('agentDefinition.current')} />
              ) : null}
              {state.phase === 'review' ? (
                <>
                  <NativeButton
                    testID="agent-definition-apply"
                    label={t('agentDefinition.apply')}
                    onPress={props.onApply}
                  />
                  <NativeButton
                    testID="agent-definition-edit"
                    secondary
                    label={t('agentDefinition.edit')}
                    onPress={props.onEdit}
                  />
                </>
              ) : null}
              {held ? (
                <>
                  {state.error ? (
                    <Text accessibilityRole="alert" style={{ ...paragraph, color: p.destructive }}>
                      {state.error === 'dismiss'
                        ? t('agentDefinition.dismissError')
                        : t('agentDefinition.resultError')}
                    </Text>
                  ) : null}
                  <NativeButton
                    testID="agent-definition-reconcile"
                    label={
                      state.reconciling ? t('agentDefinition.checking') : t('agentDefinition.check')
                    }
                    disabled={state.reconciling || state.dismissing}
                    onPress={props.onReconcile}
                  />
                  <NativeButton
                    testID="agent-definition-dismiss"
                    secondary
                    label={t('agentDefinition.another')}
                    disabled={!state.receiptKnown || state.reconciling || state.dismissing}
                    onPress={props.onDismissAttempt}
                  />
                </>
              ) : null}
            </>
          ) : null}
          <Text style={paragraph}>{t('agentDefinition.noPermissions')}</Text>
          <NativeButton secondary label={t('agentDefinition.done')} onPress={props.onDone} />
        </>
      )}
    </SettingsScreen>
  );
}
