import { useMemo, useState } from 'react';
import { Text, View } from 'react-native';
import { Image } from 'expo-image';
import * as Clipboard from 'expo-clipboard';
import qrcode from 'qrcode-generator';
import { useTheme } from '../theme';
import { NativeButton } from './primitives';

export function PairingCode({ code, check }: { code: string; check: string }) {
  const { colors } = useTheme();
  const [copied, setCopied] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const uri = useMemo(() => {
    const qr = qrcode(0, 'M');
    qr.addData(code);
    qr.make();
    return qr.createDataURL(4, 16);
  }, [code]);
  return (
    <View style={{ gap: 18, alignItems: 'center' }}>
      <Image
        source={{ uri }}
        accessibilityLabel="QR code para conectar este dispositivo"
        style={{ width: 248, height: 248, backgroundColor: '#FFFFFF', borderRadius: 16 }}
        contentFit="contain"
      />
      <Text
        selectable
        accessibilityLabel={`Código de verificação: ${check.split('').join(' ')}`}
        style={{ fontSize: 36, fontWeight: '600', letterSpacing: 8, color: colors.text }}
      >
        {check}
      </Text>
      <NativeButton
        secondary
        label={copied ? 'Código copiado' : 'Copiar código de conexão'}
        onPress={() => {
          void Clipboard.setStringAsync(code, { android: { isSensitive: true } })
            .then((saved) => {
              setCopied(saved);
              setFailure(saved ? null : 'Não foi possível copiar.');
            })
            .catch(() => setFailure('Não foi possível copiar.'));
        }}
      />
      {failure ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {failure}
        </Text>
      ) : null}
    </View>
  );
}
