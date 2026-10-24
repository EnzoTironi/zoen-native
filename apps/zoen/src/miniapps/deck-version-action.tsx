import { useCallback, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import { Text, View } from 'react-native';
import { NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import type { ItemDetail } from '../core/types';
import { DocumentExportActivity } from '../documents/export-activity';
import { t } from '../i18n/index';
import { useTheme } from '../theme';
import {
  currentDeckVersion,
  deckVersionBinding,
  observeDeckVersion,
  type DeckVersionBinding,
} from './deck-version-selection';

export function DeckVersionAction({ item }: { item: ItemDetail }) {
  const { store } = useClient();
  const binding = deckVersionBinding(store, item);
  return binding ? <VersionAction key={JSON.stringify(binding)} binding={binding} /> : null;
}
function VersionAction({ binding }: { binding: DeckVersionBinding }) {
  const { store } = useClient();
  const { colors } = useTheme();
  const [activity] = useState(() => new DocumentExportActivity());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useFocusEffect(
    useCallback(() => {
      setBusy(false);
      setError(null);
      return activity.focus();
    }, [activity]),
  );
  return (
    <View style={{ gap: 6 }}>
      <NativeButton
        secondary
        testID="deck-version-open"
        disabled={busy}
        label={t('documents.exportVersionNumber', { version: binding.version })}
        onPress={() => {
          if (!currentDeckVersion(store, binding)) return;
          const activation = activity.begin();
          if (!activation) return;
          setBusy(true);
          setError(null);
          void observeDeckVersion(store, binding, activation.current)
            .then((head) => {
              if (head && activation.current() && currentDeckVersion(store, binding))
                router.push({
                  pathname: '/miniapp/[id]',
                  params: { id: binding.itemId, version: head.version, sha256: head.sha256 },
                });
            })
            .catch(() => {
              if (activation.current() && currentDeckVersion(store, binding))
                setError(t('documents.thisVersionCouldNotBeLoaded'));
            })
            .finally(() => {
              if (activation.finish() && currentDeckVersion(store, binding)) setBusy(false);
            });
        }}
      />
      {error ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
    </View>
  );
}
