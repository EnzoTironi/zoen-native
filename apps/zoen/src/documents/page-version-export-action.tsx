import { useCallback, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import { DocumentExportActivity } from './export-activity';
import { Text, View } from 'react-native';
import { NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import type { PageDto } from '../core/generated/PageDto';
import { t } from '../i18n/index';
import { useTheme } from '../theme';
import { exportMarkdown } from './markdown-files';
import {
  currentPageExport,
  exportObservedPage,
  pageExportBinding,
  type PageExportBinding,
} from './page-version-export';

export function PageVersionExportAction({ page }: { page: PageDto }) {
  const { store } = useClient();
  const binding = pageExportBinding(store, page);
  return binding ? <ExportAction key={JSON.stringify(binding)} binding={binding} /> : null;
}
function ExportAction({ binding }: { binding: PageExportBinding }) {
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
  const current = () => currentPageExport(store, binding);
  return (
    <View style={{ gap: 6 }}>
      <NativeButton
        secondary
        disabled={busy}
        testID="page-version-export"
        label={
          busy
            ? t('documents.exportingVersion')
            : t('documents.exportVersionNumber', { version: binding.version })
        }
        onPress={() => {
          if (!current()) return;
          const activation = activity.begin();
          if (!activation) return;
          setBusy(true);
          setError(null);
          void exportObservedPage(store, binding, activation.current, exportMarkdown)
            .catch((reason: unknown) => {
              if (activation.current() && current())
                setError(
                  reason instanceof Error
                    ? reason.message
                    : t('documents.thisVersionCouldNotBeLoaded'),
                );
            })
            .finally(() => {
              if (activation.finish() && current()) setBusy(false);
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
