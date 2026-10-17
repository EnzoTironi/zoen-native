import { useState } from 'react';
import { Stack } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { useTheme } from '../theme';
import { useDialogs } from '../components/dialogs';
import { EmptyState, Icon, Loading, NativeButton, Surface } from '../components/primitives';
import { AuthorizeDevice } from '../components/authorize-device';
import { HistoryTransfer } from '../components/history-transfer';

export default function Devices() {
  const { store, mode, snapshot } = useClient();
  const { data, error, retry } = useCoreQuery(mode === 'account' ? { op: 'devices' } : null);
  const { colors } = useTheme();
  const { confirm } = useDialogs();
  const [busy, setBusy] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  async function unlink(device: string) {
    if (busy) return;
    setBusy(device);
    setFailure(null);
    try {
      const accepted = await confirm({
        title: 'Desvincular este dispositivo?',
        message:
          'Ele perderá o acesso à conta e será removido das conversas criptografadas. Os arquivos que já baixou podem continuar nele.',
        confirmLabel: 'Desvincular',
        destructive: true,
      });
      if (!accepted) return;
      await store.driver.request({ op: 'unlink_device', device_id: device });
      retry();
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : 'Não foi possível desvincular.');
    } finally {
      setBusy(null);
    }
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ padding: 24, gap: 20 }}
    >
      <Stack.Screen options={{ title: 'Dispositivos' }} />
      {mode === 'account' ? <AuthorizeDevice onLinked={retry} /> : null}
      {mode === 'account' ? (
        <HistoryTransfer
          key={`${snapshot?.account?.identity_id}:${snapshot?.account?.device_id}`}
        />
      ) : null}
      <Text style={{ color: colors.secondary, lineHeight: 24, fontSize: 16 }}>
        Confira os dispositivos autorizados a acessar sua conta. Revogue o acesso de um dispositivo
        que você não reconhece.
      </Text>
      {failure || error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
          {failure ?? error}
        </Text>
      ) : null}
      {mode === 'demo' ? (
        <EmptyState
          title="Demonstração local"
          detail="Os dispositivos vinculados aparecem quando você usa uma conta."
        />
      ) : !data && !error ? (
        <Loading />
      ) : (
        data?.map((device) => (
          <Surface key={device.device_id}>
            <View style={{ flexDirection: 'row', gap: 14, alignItems: 'center' }}>
              <Icon
                name={{
                  ios: device.this_device ? 'iphone' : 'laptopcomputer',
                  android: 'devices',
                  web: 'devices',
                }}
                size={28}
                color={device.revoked ? colors.tertiary : colors.accent}
              />
              <View style={{ flex: 1, gap: 5 }}>
                <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
                  {device.this_device
                    ? 'Este dispositivo'
                    : `Dispositivo · ${device.device_id.slice(-8)}`}
                </Text>
                <Text style={{ color: device.revoked ? colors.danger : colors.secondary }}>
                  {device.revoked ? 'Acesso revogado' : 'Autorizado'}
                </Text>
              </View>
            </View>
            <Text selectable style={{ color: colors.tertiary, fontSize: 12, lineHeight: 18 }}>
              {device.device_id}
            </Text>
            {!device.this_device && !device.revoked ? (
              <NativeButton
                secondary
                label={busy === device.device_id ? 'Aguarde…' : 'Desvincular'}
                disabled={busy !== null}
                onPress={() => {
                  void unlink(device.device_id);
                }}
              />
            ) : null}
          </Surface>
        ))
      )}
      {mode === 'account' ? (
        <NativeButton
          secondary
          label="Atualizar dispositivos"
          disabled={busy !== null}
          onPress={retry}
        />
      ) : null}
    </ScrollView>
  );
}
