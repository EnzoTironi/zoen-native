import { t } from '../i18n';
import { useEffect, useRef, useState } from 'react';
import { Text, View } from 'react-native';
import { CameraView, useCameraPermissions } from 'expo-camera';
import { useClient } from '../core/provider';
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
  const [error, setError] = useState<string | null>(null);
  const scanning = useRef(false);
  useEffect(() => {
    const subscription = CameraView.onModernBarcodeScanned((result) => {
      if (!scanning.current) return;
      scanning.current = false;
      setCode(result.data);
      setCheck(null);
      void CameraView.dismissScanner();
    });
    return () => {
      subscription.remove();
      if (scanning.current) void CameraView.dismissScanner();
    };
  }, []);
  async function run(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : t('access.connectDeviceFailed'));
    } finally {
      setBusy(false);
    }
  }
  async function transfer() {
    if (!device) return;
    const [done, total] = await store.driver.request({ op: 'link_progress', device_id: device });
    if (done < total) {
      setProgress(t('access.conversationsPrepared', { done, count: total }));
      return;
    }
    setProgress(t('access.sendingEncryptedHistory'));
    const count = await store.driver.request({ op: 'send_history', device_id: device });
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
            disabled={busy}
            onPress={() => {
              void run(transfer);
            }}
          />
          <NativeButton
            secondary
            label={t('access.connectAnotherDevice')}
            disabled={busy}
            onPress={() => {
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
              disabled={busy}
              onPress={() => {
                void run(async () => {
                  const access = permission?.granted ? permission : await requestPermission();
                  if (!access.granted) throw new Error(t('access.cameraRequired'));
                  scanning.current = true;
                  await CameraView.launchScanner({ barcodeTypes: ['qr'] });
                });
              }}
            />
          ) : null}
          <Field
            label={t('access.connectionCode')}
            value={code}
            onChangeText={(value) => {
              setCode(value);
              setCheck(null);
            }}
            editable={!busy}
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
            disabled={busy || !code.trim()}
            onPress={() => {
              void run(async () => {
                if (!check) {
                  setCheck(
                    await store.driver.request({ op: 'link_code_check', code: code.trim() }),
                  );
                  return;
                }
                if (
                  !(await confirm({
                    title: t('access.authorizeTitle'),
                    message: t('access.authorizeExplanation', { code: check }),
                    confirmLabel: t('access.authorizeDevice'),
                  }))
                )
                  return;
                const linked = await store.driver.request({ op: 'link_device', code: code.trim() });
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
        <Text accessibilityRole="status" style={{ color: colors.secondary, lineHeight: 23 }}>
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
