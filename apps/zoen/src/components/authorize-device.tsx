/* oxlint-disable jsx-a11y/prefer-tag-over-role -- React Native Text supports role="status"; HTML output is not a native component. */
import { t } from '../i18n';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AppState, Text, View } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { CameraView, useCameraPermissions } from 'expo-camera';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { useDialogs } from './dialogs';
import { useTheme } from '../theme';
import { Field, NativeButton, Surface } from './primitives';

export function AuthorizeDevice({ onLinked }: { onLinked: () => void }) {
  const { store } = useClient();
  const { colors } = useTheme();
  const { confirm } = useDialogs();
  const [permission, requestPermission] = useCameraPermissions();
  const [code, setCode] = useState('');
  const [check, setCheck] = useState<string | null>(null);
  const [device, setDevice] = useState<string | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [available, setAvailable] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const busyRef = useRef(false);
  const scanning = useRef<number | null>(null);
  const lease = useRef({ generation: 0, owner: '', active: false });
  const owner = accountScopeKey(accountScope(store));
  const current = useCallback(
    (generation = lease.current.generation) =>
      lease.current.active &&
      generation === lease.current.generation &&
      lease.current.owner === accountScopeKey(accountScope(store)),
    [store],
  );
  useFocusEffect(
    useCallback(() => {
      const retire = () => {
        lease.current = { generation: lease.current.generation + 1, owner: '', active: false };
        busyRef.current = false;
        if (scanning.current !== null) void CameraView.dismissScanner().catch(() => {});
        scanning.current = null;
        setAvailable(false);
        setBusy(false);
        setCode('');
        setCheck(null);
        setDevice(null);
        setProgress(null);
        setError(null);
      };
      const activate = () => {
        retire();
        if (AppState.currentState !== 'active' || !owner) return;
        if (owner !== accountScopeKey(accountScope(store))) return;
        lease.current = { generation: lease.current.generation, owner, active: true };
        setAvailable(true);
      };
      activate();
      const preflight = store.registerSessionPreflight(retire);
      const unsubscribe = store.subscribe(() => {
        if (!current()) retire();
      });
      const app = AppState.addEventListener('change', (next) => {
        if (next === 'active') activate();
        else retire();
      });
      return () => {
        preflight();
        unsubscribe();
        app.remove();
        retire();
      };
    }, [store, owner, current]),
  );
  useEffect(() => {
    const subscription = CameraView.onModernBarcodeScanned((result) => {
      const generation = scanning.current;
      if (generation === null || !current(generation)) return;
      scanning.current = null;
      setCode(result.data);
      setCheck(null);
      void CameraView.dismissScanner().catch(() => {});
    });
    return () => {
      subscription.remove();
      if (scanning.current !== null) void CameraView.dismissScanner().catch(() => {});
      scanning.current = null;
    };
  }, [current]);
  const renderedGeneration = lease.current.generation;
  async function run(action: (generation: number) => Promise<void>) {
    if (busyRef.current || !current(renderedGeneration)) return;
    const generation = renderedGeneration;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    try {
      await action(generation);
    } catch (reason) {
      if (current(generation))
        setError(reason instanceof Error ? reason.message : t('access.connectDeviceFailed'));
    } finally {
      if (current(generation)) {
        busyRef.current = false;
        setBusy(false);
      }
    }
  }
  async function transfer(generation: number) {
    if (!device || !current(generation)) return;
    const [done, total] = await store.driver.request({ op: 'link_progress', device_id: device });
    if (!current(generation)) return;
    if (done < total) {
      setProgress(t('access.conversationsPrepared', { done, count: total }));
      return;
    }
    setProgress(t('access.sendingEncryptedHistory'));
    const count = await store.driver.request({ op: 'send_history', device_id: device });
    if (!current(generation)) return;
    setProgress(t('access.eventsSent', { count }));
    onLinked();
  }
  return (
    <Surface>
      <Text style={{ color: colors.text, fontSize: 19, fontWeight: '600' }}>
        {t('access.connectDevice')}
      </Text>
      {device ? (
        <>
          <Text style={{ color: colors.secondary, lineHeight: 24 }}>
            {t('access.authorizedSendHistory')}
          </Text>
          <NativeButton
            label={busy ? t('access.wait') : t('access.sendHistory')}
            disabled={busy || !available}
            onPress={() => {
              void run(transfer);
            }}
          />
          <NativeButton
            secondary
            label={t('access.connectAnotherDevice')}
            disabled={busy || !available}
            onPress={() => {
              if (!current(renderedGeneration) || busyRef.current) return;
              setDevice(null);
              setCode('');
              setCheck(null);
              setProgress(null);
            }}
          />
        </>
      ) : (
        <>
          <Text style={{ color: colors.secondary, lineHeight: 24 }}>
            {t('access.scanOrPasteCode')}
          </Text>
          {CameraView.isModernBarcodeScannerAvailable ? (
            <NativeButton
              secondary
              label={t('access.scanQr')}
              disabled={busy || !available}
              onPress={() => {
                void run(async (generation) => {
                  const access = permission?.granted ? permission : await requestPermission();
                  if (!current(generation)) return;
                  if (!access.granted) throw new Error(t('access.cameraRequired'));
                  scanning.current = generation;
                  await CameraView.launchScanner({ barcodeTypes: ['qr'] });
                  if (!current(generation) && scanning.current === null)
                    void CameraView.dismissScanner().catch(() => {});
                });
              }}
            />
          ) : null}
          <Field
            label={t('access.connectionCode')}
            value={code}
            onChangeText={(value) => {
              if (!current(renderedGeneration) || busyRef.current) return;
              setCode(value);
              setCheck(null);
            }}
            editable={!busy && available}
            autoCapitalize="none"
            autoCorrect={false}
            multiline
          />
          {check ? (
            <View style={{ gap: 10 }}>
              <Text
                selectable
                style={{
                  color: colors.text,
                  fontSize: 36,
                  fontWeight: '600',
                  letterSpacing: 8,
                  textAlign: 'center',
                }}
              >
                {check}
              </Text>
              <Text style={{ color: colors.secondary, lineHeight: 24 }}>
                {t('access.compareSixDigits')}
              </Text>
            </View>
          ) : null}
          <NativeButton
            label={
              busy
                ? t('access.wait')
                : check
                  ? t('access.matchingDigitsAuthorize')
                  : t('access.checkCode')
            }
            disabled={busy || !available || !code.trim()}
            onPress={() => {
              void run(async (generation) => {
                if (!check) {
                  const checked = await store.driver.request({
                    op: 'link_code_check',
                    code: code.trim(),
                  });
                  if (current(generation)) setCheck(checked);
                  return;
                }
                const accepted = await confirm({
                  title: t('access.authorizeTitle'),
                  message: t('access.authorizeExplanation', { code: check }),
                  confirmLabel: t('access.authorizeDevice'),
                });
                if (!accepted || !current(generation)) return;
                const linked = await store.driver.request({ op: 'link_device', code: code.trim() });
                if (!current(generation)) return;
                setDevice(linked.device_id);
                setCode('');
                setCheck(null);
                onLinked();
              });
            }}
          />
        </>
      )}
      {progress ? (
        <Text
          role="status"
          accessibilityLiveRegion="polite"
          style={{ color: colors.secondary, lineHeight: 23 }}
        >
          {progress}
        </Text>
      ) : null}
      {error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 23 }}>
          {error}
        </Text>
      ) : null}
    </Surface>
  );
}
