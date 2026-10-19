/* oxlint-disable jsx-a11y/prefer-tag-over-role -- React Native Text supports role="status"; HTML output is not a native component. */
import { t, formatLocale } from '../i18n';
import { useRef, useState } from 'react';
import { Stack, usePreventRemove } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { useTheme } from '../theme';
import { useDialogs } from '../components/dialogs';
import { EmptyState, Field, NativeButton, Surface } from '../components/primitives';

export default function Backup() {
  const { store, mode } = useClient();
  const { data, error, retry } = useCoreQuery(mode === 'account' ? { op: 'backup_status' } : null);
  const { colors } = useTheme();
  const { confirm, inform } = useDialogs();
  const [key, setKey] = useState('');
  const [saved, setSaved] = useState(false);
  const [password, setPassword] = useState('');
  const [repeat, setRepeat] = useState('');
  const [method, setMethod] = useState<'recovery' | 'password'>('recovery');
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [leaving, setLeaving] = useState(false);
  const confirming = useRef(false);
  usePreventRemove(!leaving && (!!key || !!password || busy), ({ repeat: leave }) => {
    if (busy) {
      void inform(t('access.backupInProgress'), t('access.waitBeforeLeaving'));
      return;
    }
    if (confirming.current) return;
    confirming.current = true;
    void confirm({
      title: t('access.leaveBackupTitle'),
      message: key ? t('access.recoveryKeyWillHide') : t('access.passwordWillDiscard'),
      confirmLabel: t('access.leaveSetup'),
      cancelLabel: t('access.stayHere'),
    })
      .then((accepted) => {
        if (accepted) {
          setLeaving(true);
          leave();
        }
      })
      .finally(() => {
        confirming.current = false;
      });
  });
  async function perform(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    setStatus(null);
    try {
      await action();
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : t('access.backupFailed'));
    } finally {
      retry();
      setBusy(false);
    }
  }
  if (mode === 'demo')
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
          {data
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
              label={busy ? t('access.wait') : t('access.backupNow')}
              disabled={busy}
              onPress={() => {
                void perform(async () => {
                  await store.driver.request({ op: 'backup_now' });
                  setStatus(t('access.backupSentProtected'));
                });
              }}
            />
            <NativeButton
              secondary
              label={t('access.disableDeleteBackup')}
              disabled={busy}
              onPress={() => {
                void perform(async () => {
                  if (
                    !(await confirm({
                      title: t('access.deleteServerBackupTitle'),
                      message: t('access.deleteServerBackupExplanation'),
                      confirmLabel: t('access.deleteBackup'),
                      destructive: true,
                    }))
                  )
                    return;
                  await store.driver.request({ op: 'backup_turn_off' });
                  setStatus(t('access.backupDeleted'));
                });
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
              label={t('access.recoveryKey')}
              secondary={method !== 'recovery'}
              disabled={busy || !!key}
              onPress={() => setMethod('recovery')}
            />
            <NativeButton
              label={t('access.backupPassword')}
              secondary={method !== 'password'}
              disabled={busy || !!key}
              onPress={() => setMethod('password')}
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
                  {key.match(/.{1,4}/g)?.join(' ')}
                </Text>
                <NativeButton
                  secondary
                  label={t('access.copyRecoveryKey')}
                  onPress={() => {
                    void Clipboard.setStringAsync(key, { android: { isSensitive: true } })
                      .then((copied) => {
                        if (!copied) throw new Error(t('access.copyFailed'));
                        setStatus(t('access.keyCopiedStoreSafely'));
                      })
                      .catch(() => setFailure(t('access.copyKeyFailed')));
                  }}
                />
                <Pressable
                  accessibilityRole="checkbox"
                  accessibilityState={{ checked: saved }}
                  disabled={busy}
                  onPress={() => setSaved((value) => !value)}
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
                  label={busy ? t('access.protectingBackup') : t('access.enableBackup')}
                  disabled={busy || !saved}
                  onPress={() => {
                    void perform(async () => {
                      await store.driver.request({
                        op: 'backup_enable_recovery_key',
                        recovery_key: key,
                      });
                      setKey('');
                      setSaved(false);
                      setStatus(t('access.backupEnabledSent'));
                    });
                  }}
                />
              </>
            ) : (
              <NativeButton
                label={t('access.generateKey')}
                disabled={busy}
                onPress={() => {
                  void perform(async () => {
                    setKey(await store.driver.request({ op: 'backup_create_recovery_key' }));
                    setSaved(false);
                  });
                }}
              />
            )
          ) : (
            <>
              <Field
                label={t('access.newBackupPassword')}
                value={password}
                onChangeText={setPassword}
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
                onChangeText={setRepeat}
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
                label={busy ? t('access.protectingBackup') : t('access.enableWithPassword')}
                disabled={busy || password.length < 12 || password !== repeat}
                onPress={() => {
                  void perform(async () => {
                    await store.driver.request({ op: 'backup_password', password });
                    setPassword('');
                    setRepeat('');
                    setStatus(t('access.backupEnabledSent'));
                  });
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
          {status}
        </Text>
      ) : null}
      {failure || error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 23 }}>
          {failure ?? error}
        </Text>
      ) : null}
      {error ? (
        <NativeButton secondary label={t('access.retry')} disabled={busy} onPress={retry} />
      ) : null}
    </ScrollView>
  );
}
