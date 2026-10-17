import { useState } from 'react';
import { Stack, useLocalSearchParams } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { Text, View } from 'react-native';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import { useIntegrityReport } from '../../integrity/use-integrity-report';
import type { LogEventDto } from '../../core/generated/LogEventDto';
import { Loading, NativeButton, Surface } from '../../components/primitives';
import { useTheme } from '../../theme';
import { t, formatLocale } from '../../i18n';

export default function Integrity() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { snapshot, store } = useClient();
  const { colors } = useTheme();
  const report = useIntegrityReport(id);
  const audit = useCoreQuery({ op: 'log_events_page', space_id: id, before: null, limit: 50 });
  const scope = `${snapshot?.account?.identity_id ?? ''}:${id}`;
  const base = `${scope}:${JSON.stringify(audit.data)}`;
  const [more, setMore] = useState<{
    base: string;
    events: LogEventDto[];
    before: string | null;
  } | null>(null);
  const [failure, setFailure] = useState<{ scope: string; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const extra = more?.base === base ? more : null;
  const entries = [...(audit.data?.events ?? []), ...(extra?.events ?? [])];
  const before = extra ? extra.before : audit.data?.before;
  const error = report.error ?? audit.error ?? (failure?.scope === scope ? failure.text : null);
  const valid = !report.error && report.data?.valid === true;
  return (
    <View style={{ flex: 1, backgroundColor: colors.background }}>
      <Stack.Screen options={{ title: t('integrity.title') }} />
      <FlashList
        data={entries}
        keyExtractor={(event) => event.hash}
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={{ padding: 20 }}
        ListHeaderComponent={
          <View style={{ gap: 18, paddingBottom: 18 }}>
            {!report.data && !report.error ? <Loading /> : null}
            <Surface>
              <Text
                accessibilityRole={valid ? 'header' : 'alert'}
                style={{
                  color: valid ? colors.text : colors.danger,
                  fontSize: 20,
                  fontWeight: '600',
                }}
              >
                {report.error
                  ? t('integrity.unavailable')
                  : !report.data
                    ? t('integrity.verifying')
                    : valid
                      ? t('integrity.verified')
                      : t('integrity.needsAttention')}
              </Text>
              <Text style={{ color: colors.secondary }}>{report.data?.space_title}</Text>
              {valid ? (
                <Text selectable style={{ color: colors.secondary }}>
                  {t('integrity.verifiedHead', {
                    events: report.data?.events ?? '0',
                    hash: report.data?.head_hash ?? '',
                  })}
                </Text>
              ) : null}
              {report.data && !valid ? (
                <Text selectable style={{ color: colors.danger }}>
                  {report.data.error ?? t('integrity.checkDevice')}
                </Text>
              ) : null}
              <Text style={{ color: colors.secondary, lineHeight: 22 }}>
                {t('integrity.explanation')}
              </Text>
              <NativeButton
                secondary
                label={t('integrity.verifyAgain')}
                onPress={() => {
                  report.retry();
                  audit.retry();
                  setMore(null);
                  setFailure(null);
                }}
              />
            </Surface>
            {error ? (
              <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
                {error}
              </Text>
            ) : null}
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
            >
              {t('integrity.localEvents')}
            </Text>
          </View>
        }
        renderItem={({ item }) => (
          <View
            style={{ gap: 5, paddingVertical: 15, borderBottomWidth: 1, borderColor: colors.line }}
          >
            <Text style={{ color: colors.text, fontWeight: '600' }}>
              #{item.seq} · {item.label}
            </Text>
            <Text style={{ color: colors.secondary }}>
              {item.author.name} · {new Date(Number(item.at_ms)).toLocaleString(formatLocale)}
            </Text>
            <Text
              selectable
              style={{ color: colors.secondary, fontFamily: 'monospace', fontSize: 12 }}
            >
              {t('integrity.hash', { hash: item.hash })}
              {'\n'}
              {t('integrity.previous', { hash: item.prev })}
              {'\n'}
              {t('integrity.signature', { signature: item.signature })}
            </Text>
          </View>
        )}
        ListFooterComponent={
          before ? (
            <View style={{ paddingVertical: 20 }}>
              <NativeButton
                secondary
                disabled={busy}
                label={busy ? t('integrity.loading') : t('integrity.earlierEvents')}
                onPress={() => {
                  setBusy(true);
                  setFailure(null);
                  void store.driver
                    .request({ op: 'log_events_page', space_id: id, before, limit: 50 })
                    .then((page) => {
                      if (
                        store.getSnapshot().snapshot?.account?.identity_id !==
                        snapshot?.account?.identity_id
                      )
                        return;
                      setMore({
                        base,
                        events: [...(extra?.events ?? []), ...page.events],
                        before: page.before,
                      });
                    })
                    .catch((reason: unknown) =>
                      setFailure({
                        scope,
                        text: reason instanceof Error ? reason.message : t('integrity.loadFailed'),
                      }),
                    )
                    .finally(() => setBusy(false));
                }}
              />
            </View>
          ) : null
        }
      />
    </View>
  );
}
