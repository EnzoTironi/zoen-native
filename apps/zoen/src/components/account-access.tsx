/* oxlint-disable jsx-a11y/prefer-tag-over-role -- React Native Text supports role="status"; HTML output is not a native component. */
import { t } from '../i18n';
import { useState } from 'react';
import { Text, View } from 'react-native';
import { useClient } from '../core/provider';
import type { LinkRequestDto } from '../core/generated/LinkRequestDto';
import { useTheme } from '../theme';
import { Field, NativeButton, Surface } from './primitives';
import { PairingCode } from './pairing-code';

const message = (error: unknown) =>
  error instanceof Error ? error.message : t('access.finishFailed');

export function AccountAccess({
  relay,
  expectedIdentityId,
  expectedHandle,
  onDone,
  onBack,
}: {
  relay: string;
  expectedIdentityId?: string;
  expectedHandle?: string;
  onDone: () => void;
  onBack: () => void;
}) {
  const { store } = useClient();
  const { colors } = useTheme();
  const [method, setMethod] = useState<'link' | 'recovery' | 'password'>('link');
  const [request, setRequest] = useState<LinkRequestDto | null>(null);
  const [handle, setHandle] = useState(expectedHandle ?? '');
  const [secret, setSecret] = useState('');
  const [busy, setBusy] = useState(false);
  const [linked, setLinked] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  async function run(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  }
  async function history() {
    const current = await store.driver.request({ op: 'history_status' });
    if (current.receive_active) {
      setStatus(t('access.historyReceivingBackground'));
      return;
    }
    if (!current.initial_pending) {
      setStatus(t('access.historyAlreadyReceived'));
      return;
    }
    setStatus(t('access.receivingHistory'));
    const result = await store.driver.request({ op: 'receive_history', timeout_ms: 15_000 });
    setStatus(t('access.historyReceived', { count: result.messages }));
    await store.refresh();
  }
  return (
    <View style={{ gap: 20 }}>
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 28, fontWeight: '600' }}
      >
        {t('access.accountOnDevice')}
      </Text>
      {expectedHandle ? (
        <Text style={{ color: colors.secondary, lineHeight: 24 }}>
          {t('access.recognizedAccount', { handle: expectedHandle })}
        </Text>
      ) : null}
      {!linked ? (
        <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
          {(
            [
              { value: 'link', label: t('access.otherDevice') },
              { value: 'recovery', label: t('access.recoveryKey') },
              { value: 'password', label: t('access.backupPassword') },
            ] as const
          ).map((choice) => (
            <NativeButton
              key={choice.value}
              secondary={method !== choice.value}
              disabled={busy}
              label={choice.label}
              onPress={() => {
                setMethod(choice.value);
                setSecret('');
                setError(null);
              }}
            />
          ))}
        </View>
      ) : null}
      {linked ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
            {t('access.accountConnected')}
          </Text>
          <Text style={{ color: colors.secondary, lineHeight: 24 }}>
            {t('access.connectedKeepPeerOnline')}
          </Text>
          <NativeButton
            label={t('access.receiveHistory')}
            disabled={busy}
            onPress={() => {
              void run(history);
            }}
          />
          <NativeButton
            secondary
            label={t('access.enterZoen')}
            disabled={busy}
            onPress={() => {
              void run(async () => {
                await store.refresh();
                onDone();
              });
            }}
          />
        </Surface>
      ) : method === 'link' ? (
        <>
          <Text style={{ color: colors.secondary, lineHeight: 24 }}>
            {t('access.linkInstructions')}
          </Text>
          {request ? <PairingCode code={request.code} check={request.check} /> : null}
          <NativeButton
            label={
              busy
                ? t('access.waitingApproval')
                : request
                  ? t('access.alreadyApproved')
                  : t('access.generateCode')
            }
            disabled={busy || !relay.trim()}
            onPress={() => {
              void run(async () => {
                if (!request) {
                  setRequest(
                    await store.driver.request({ op: 'link_request', relay_url: relay.trim() }),
                  );
                  return;
                }
                await store.driver.request({
                  op: 'link_wait',
                  timeout_ms: 10_000,
                  expected_identity_id: expectedIdentityId ?? null,
                });
                setLinked(true);
                setStatus(t('access.connectedSendHistory'));
                await store.refresh();
              });
            }}
          />
        </>
      ) : (
        <>
          <Text style={{ color: colors.secondary, lineHeight: 24 }}>
            {method === 'recovery' ? t('access.restoreUsingKey') : t('access.restoreUsingPassword')}
          </Text>
          <Field
            label={t('profile.handle')}
            value={handle}
            onChangeText={setHandle}
            editable={!busy && !expectedHandle}
            autoCapitalize="none"
            autoCorrect={false}
          />
          <Field
            label={method === 'recovery' ? t('access.recoveryKey') : t('access.backupPassword')}
            value={secret}
            onChangeText={setSecret}
            editable={!busy}
            secureTextEntry
            autoCapitalize="none"
            autoCorrect={false}
            autoComplete="off"
          />
          <NativeButton
            label={busy ? t('access.restoringBackup') : t('access.restoreAccount')}
            disabled={busy || !handle.trim() || !secret.trim() || !relay.trim()}
            onPress={() => {
              void run(async () => {
                const common = {
                  relay_url: relay.trim(),
                  handle: handle.trim().replace(/^@/, ''),
                  expected_identity_id: expectedIdentityId ?? null,
                };
                await store.driver.request(
                  method === 'recovery'
                    ? {
                        op: 'restore_backup_recovery_key',
                        ...common,
                        recovery_key: secret.replace(/\s/g, ''),
                      }
                    : { op: 'restore_backup_password', ...common, password: secret },
                );
                setSecret('');
                await store.refresh();
                onDone();
              });
            }}
          />
        </>
      )}
      {status ? (
        <Text
          role="status"
          accessibilityLiveRegion="polite"
          style={{ color: colors.secondary, lineHeight: 23 }}
        >
          {status}
        </Text>
      ) : null}
      {error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 23 }}>
          {error}
        </Text>
      ) : null}
      {!linked ? (
        <NativeButton secondary label={t('access.back')} disabled={busy} onPress={onBack} />
      ) : null}
    </View>
  );
}
