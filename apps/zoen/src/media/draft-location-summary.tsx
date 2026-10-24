import { useEffect, useState } from 'react';
import { Text } from 'react-native';
import { File, FileMode } from 'expo-file-system';
import { digestStringAsync, CryptoDigestAlgorithm, CryptoEncoding } from 'expo-crypto';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { readDraftLocation } from './draft-location-read';
import type { StaticLocation } from './types';

export function DraftLocationSummary({ uri, sha256 }: { uri: string; sha256: string }) {
  const { colors } = useTheme();
  const [result, setResult] = useState<{
    uri: string;
    sha256: string;
    point: StaticLocation;
  } | null>(null);
  useEffect(() => {
    let current = true;
    void readDraftLocation(
      { uri, sha256 },
      {
        size: (path) => new File(path).size,
        bytes: async (path, maximum) => {
          const handle = new File(path).open(FileMode.ReadOnly);
          try {
            return await handle.readBytes(maximum);
          } finally {
            handle.close();
          }
        },
        sha256: (text) =>
          digestStringAsync(CryptoDigestAlgorithm.SHA256, text, { encoding: CryptoEncoding.HEX }),
      },
      () => current,
    )
      .then((point) => {
        if (current && point) setResult({ uri, sha256, point });
      })
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [uri, sha256]);
  const point = result?.uri === uri && result.sha256 === sha256 ? result.point : null;
  return (
    <Text
      numberOfLines={3}
      style={{ color: colors.text, fontSize: 10, textAlign: 'center', padding: 4 }}
    >
      {point
        ? (point.label ?? `${point.latitude}, ${point.longitude}`)
        : t('attachmentTabs.location')}
    </Text>
  );
}
