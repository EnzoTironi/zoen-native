/* oxlint-disable jsx-a11y/prefer-tag-over-role -- React Native Text supports role="status"; HTML output is not a native component. */
import { t, formatLocale } from '../i18n';
import { Stack, usePreventRemove } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useBackup } from '../backup/use-backup';
import { useTheme } from '../theme';
import { useDialogs } from '../components/dialogs';
import { EmptyState, Field, NativeButton, Surface } from '../components/primitives';

export default function Backup() {
  const { session, state, accountRequired } = useBackup();
  const { data, key, saved, password, repeat, method, failure, status } = state;
  const busy = state.pending || !!data?.in_progress || state.phase !== 'ready';
  const error = state.phase === 'failed' ? t('access.backupFailed') : null;
  const ticket = session.ticket(state);
  const retry = () => {
    void session.load(ticket);
  };
  const { colors } = useTheme();
  const { confirm, inform } = useDialogs();
  usePreventRemove(
    !state.leaving && (!!key || !!password || state.pending || !!data?.in_progress),
    ({ repeat: leave }) => {
      if (state.pending || data?.in_progress) {
        void inform(t('access.backupInProgress'), t('access.waitBeforeLeaving'));
        return;
      }
      void session
        .leave(
          ticket,
          () =>
            confirm({
              title: t('access.leaveBackupTitle'),
              message: key ? t('access.recoveryKeyWillHide') : t('access.passwordWillDiscard'),
              confirmLabel: t('access.leaveSetup'),
              cancelLabel: t('access.stayHere'),
            }),
          leave,
        )
        .catch(() => {});
    },
  );
  if (accountRequired)
    return (
      <EmptyState title={t('access.yourAccountBackup')} detail={t('access.signInForBackup')} />
    );
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{ padding: 24, gap: 20 }}
    >
      <Stack.Screen options={{ title: t('access.backupTitle') }} />
      <Text style={{ color: colors.secondary, lineHeight: 24, fontSize: 16 }}>
        {t('access.backupExplanation')}
      </Text>
      <Surface>
        <Text style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}>
          {data?.in_progress
            ? t('access.backupInProgress')
            : data
              ? data.enabled
                ? t('access.backupEnabled')
                : t('access.backupDisabled')
              : t('access.checkingBackup')}
        </Text>
        {data?.enabled ? (
          <>
            <Text style={{ color: colors.secondary }}>
              {data.mode === 'recovery_key'
                ? t('access.protectedByKey')
                : t('access.protectedByPassword')}
            </Text>
            <Text style={{ color: colors.secondary }}>
              {data.last_backup_ms !== '0'
                ? t('access.backupLastSent', {
                    date: new Date(Number(data.last_backup_ms)).toLocaleString(formatLocale),
                  })
                : t('access.firstBackupPending')}
            </Text>
            <NativeButton
              testID="backup-now"
              label={busy ? t('access.wait') : t('access.backupNow')}
              disabled={busy}
              onPress={() => {
                void session.upload(ticket);
              }}
            />
            <NativeButton
              secondary
              testID="backup-disable"
              label={t('access.disableDeleteBackup')}
              disabled={busy}
              onPress={() => {
                void session.disable(ticket, () =>
                  confirm({
                    title: t('access.deleteServerBackupTitle'),
                    message: t('access.deleteServerBackupExplanation'),
                    confirmLabel: t('access.deleteBackup'),
                    destructive: true,
                  }),
                );
              }}
            />
          </>
        ) : null}
      </Surface>
      {data && (!data.enabled || key) ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 19, fontWeight: '600' }}>
            {t('access.howRecoverAccount')}
          </Text>
          <View style={{ flexDirection: 'row', gap: 10, flexWrap: 'wrap' }}>
            <NativeButton
              testID="backup-method-recovery"
              label={t('access.recoveryKey')}
              secondary={method !== 'recovery'}
              disabled={busy || !!key}
              onPress={() => session.change(ticket, { method: 'recovery' })}
            />
            <NativeButton
              testID="backup-method-password"
              label={t('access.backupPassword')}
              secondary={method !== 'password'}
              disabled={busy || !!key}
              onPress={() => session.change(ticket, { method: 'password' })}
            />
          </View>
          {method === 'recovery' ? (
            key ? (
              <>
                <Text style={{ color: colors.secondary, lineHeight: 24 }}>
                  {t('access.saveRecoveryKeyFirst')}
                </Text>
                <Text
                  selectable
                  style={{
                    color: colors.text,
                    fontFamily: 'monospace',
                    fontSize: 17,
                    lineHeight: 28,
                  }}
                >
                  {key}
                </Text>
                <NativeButton
                  secondary
                  testID="backup-copy"
                  label={t('access.copyRecoveryKey')}
                  disabled={state.pending || !!data?.in_progress}
                  onPress={() => {
                    void session.copy(ticket);
                  }}
                />
                <Pressable
                  testID="backup-key-saved"
                  accessibilityRole="checkbox"
                  accessibilityState={{ checked: saved }}
                  disabled={busy}
                  onPress={() => session.change(ticket, { saved: !saved })}
                  style={{
                    paddingVertical: 14,
                    flexDirection: 'row',
                    gap: 12,
                    alignItems: 'center',
                  }}
                >
                  <Text style={{ color: colors.accent, fontSize: 24 }}>{saved ? '☑' : '☐'}</Text>
                  <Text style={{ color: colors.text, flex: 1, lineHeight: 23 }}>
                    {t('access.savedKeyOutsideZoen')}
                  </Text>
                </Pressable>
                <NativeButton
                  testID="backup-enable"
                  label={busy ? t('access.protectingBackup') : t('access.enableBackup')}
                  disabled={busy || !saved}
                  onPress={() => {
                    void session.enable(ticket);
                  }}
                />
              </>
            ) : (
              <NativeButton
                testID="backup-generate"
                label={t('access.generateKey')}
                disabled={busy}
                onPress={() => {
                  void session.generate(ticket);
                }}
              />
            )
          ) : (
            <>
              <Field
                label={t('access.newBackupPassword')}
                value={password}
                onChangeText={(password) => session.change(ticket, { password })}
                secureTextEntry
                autoComplete="new-password"
                textContentType="newPassword"
                autoCapitalize="none"
                autoCorrect={false}
                editable={!busy}
              />
              <Field
                label={t('access.repeatPassword')}
                value={repeat}
                onChangeText={(repeat) => session.change(ticket, { repeat })}
                secureTextEntry
                autoComplete="new-password"
                textContentType="newPassword"
                autoCapitalize="none"
                autoCorrect={false}
                editable={!busy}
              />
              <Text style={{ color: colors.secondary, lineHeight: 22 }}>
                {t('access.passwordGuidance')}
              </Text>
              <NativeButton
                testID="backup-password-enable"
                label={busy ? t('access.protectingBackup') : t('access.enableWithPassword')}
                disabled={busy || password.length < 12 || password !== repeat}
                onPress={() => {
                  void session.password(ticket);
                }}
              />
            </>
          )}
        </Surface>
      ) : null}
      {status ? (
        <Text
          role="status"
          accessibilityLiveRegion="polite"
          style={{ color: colors.secondary, lineHeight: 23 }}
        >
          {t(
            status === 'uploaded'
              ? 'access.backupSentProtected'
              : status === 'enabled'
                ? 'access.backupEnabledSent'
                : status === 'disabled'
                  ? 'access.backupDeleted'
                  : 'access.keyCopiedStoreSafely',
          )}
        </Text>
      ) : null}
      {failure || error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 23 }}>
          {failure ? t('access.backupFailed') : error}
        </Text>
      ) : null}
      {error ? (
        <NativeButton secondary label={t('access.retry')} disabled={busy} onPress={retry} />
      ) : null}
    </ScrollView>
  );
}
