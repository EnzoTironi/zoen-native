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
      setError(
        reason instanceof Error ? reason.message : 'Não foi possível conectar o dispositivo.',
      );
    } finally {
      setBusy(false);
    }
  }
  async function transfer() {
    if (!device) return;
    const [done, total] = await store.driver.request({ op: 'link_progress', device_id: device });
    if (done < total) {
      setProgress(
        `${done} de ${total} conversas preparadas. Mantenha os dois dispositivos conectados e tente novamente em instantes.`,
      );
      return;
    }
    setProgress('Enviando o histórico criptografado…');
    const count = await store.driver.request({ op: 'send_history', device_id: device });
    setProgress(`${count} eventos enviados. O novo dispositivo já pode receber o histórico.`);
    onLinked();
  }
  return (
    <Surface>
      <Text style={{ color: colors.text, fontSize: 19, fontWeight: '600' }}>
        Conectar dispositivo
      </Text>
      {device ? (
        <>
          <Text style={{ color: colors.secondary, lineHeight: 24 }}>
            Dispositivo autorizado. Envie o histórico para que suas conversas anteriores apareçam
            nele.
          </Text>
          <NativeButton
            label={busy ? 'Aguarde…' : 'Enviar histórico'}
            disabled={busy}
            onPress={() => {
              void run(transfer);
            }}
          />
          <NativeButton
            secondary
            label="Conectar outro dispositivo"
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
            Escaneie o QR ou cole o código mostrado pelo Zoen no dispositivo que deseja conectar.
          </Text>
          {CameraView.isModernBarcodeScannerAvailable ? (
            <NativeButton
              secondary
              label="Escanear QR code"
              disabled={busy}
              onPress={() => {
                void run(async () => {
                  const access = permission?.granted ? permission : await requestPermission();
                  if (!access.granted)
                    throw new Error(
                      'Permita o uso da câmera nos Ajustes ou cole o código de conexão.',
                    );
                  scanning.current = true;
                  await CameraView.launchScanner({ barcodeTypes: ['qr'] });
                });
              }}
            />
          ) : null}
          <Field
            label="Código de conexão"
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
                Estes seis números precisam aparecer no outro dispositivo. Só autorize se os números
                forem iguais e você iniciou esta conexão.
              </Text>
            </View>
          ) : null}
          <NativeButton
            label={
              busy ? 'Aguarde…' : check ? 'Os números são iguais — autorizar' : 'Conferir código'
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
                    title: 'Autorizar este dispositivo?',
                    message: `Confirme que ${check} aparece no outro dispositivo. Ele poderá acessar sua conta e seu histórico.`,
                    confirmLabel: 'Autorizar dispositivo',
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
