import { t } from '../i18n';
import { useMemo, useState } from 'react';
import { Text, View } from 'react-native';
import { Image } from 'expo-image';
import * as Clipboard from 'expo-clipboard';
import qrcode from 'qrcode-generator';
import { useTheme } from '../theme';
import { NativeButton } from './primitives';

export function PairingCode({ code, check }: { code: string; check: string }) {
  const { colors } = useTheme();
  const [copiedCode, setCopiedCode] = useState<string | null>(null);
  const [failedCode, setFailedCode] = useState<string | null>(null);
  const [shownCode, setShownCode] = useState<string | null>(null);
  const uri = useMemo(() => {
    const qr = qrcode(0, 'M');
    qr.addData(code);
    qr.make();
    return qr.createDataURL(4, 16);
  }, [code]);
  return (
    <View style={{ gap: 18, alignItems: 'center' }}>
      <Image
        cachePolicy="none"
        source={{ uri }}
        accessibilityLabel={t('access.pairingQr')}
        style={{ width: 248, height: 248, backgroundColor: '#FFFFFF', borderRadius: 16 }}
        contentFit="contain"
      />
      <Text
        selectable
        accessibilityLabel={t('access.verificationCode', { code: check.split('').join(' ') })}
        style={{ fontSize: 36, fontWeight: '600', letterSpacing: 8, color: colors.text }}
      >
        {check}
      </Text>
      <NativeButton
        secondary
        label={copiedCode === code ? t('access.codeCopied') : t('access.copyConnectionCode')}
        onPress={() => {
          setCopiedCode(null);
          setFailedCode(null);
          void Clipboard.setStringAsync(code, { android: { isSensitive: true } })
            .then((saved) => {
              // SDK58 returns false on Web when the clipboard write is refused.
              setCopiedCode(saved === true ? code : null);
              setFailedCode(saved === true ? null : code);
              if (saved !== true) setShownCode(code);
            })
            .catch(() => {
              setCopiedCode(null);
              setFailedCode(code);
              setShownCode(code);
            });
        }}
      />
      {failedCode === code ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {t('access.copyCodeFailed')}
        </Text>
      ) : null}
      <NativeButton
        secondary
        label={shownCode === code ? t('access.hideCode') : t('access.showCode')}
        onPress={() => setShownCode((shown) => (shown === code ? null : code))}
      />
      {shownCode === code ? (
        <View style={{ alignSelf: 'stretch', gap: 8 }}>
          <Text style={{ color: colors.secondary, fontSize: 14, lineHeight: 20 }}>
            {t('access.selectConnectionCode')}
          </Text>
          <Text
            selectable
            accessibilityLabel={t('access.deviceConnectionCode')}
            style={{
              color: colors.text,
              backgroundColor: colors.muted,
              padding: 14,
              borderRadius: 14,
              fontFamily: 'monospace',
              fontSize: 13,
              lineHeight: 20,
            }}
          >
            {code}
          </Text>
        </View>
      ) : null}
    </View>
  );
}
