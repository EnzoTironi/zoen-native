import { router, useFocusEffect } from 'expo-router';
import * as Linking from 'expo-linking';
import { useCallback, useState } from 'react';
import { Text, View } from 'react-native';
import { useDialogs } from '../components/dialogs';
import { NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import type { SiteScope } from '../core/generated/SiteScope';
import type { SiteSource } from '../core/generated/SiteSource';
import type { SiteStatus } from '../core/generated/SiteStatus';
import { DocumentExportActivity } from '../documents/export-activity';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { sameSource, siteAuthority } from './source';
import { publishObservedSite } from './publication';

export function SitePublicationAction({
  saved,
  scope,
  authority,
}: {
  saved: SiteSource;
  scope: SiteScope;
  authority: string;
}) {
  const { store } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const [activity] = useState(() => new DocumentExportActivity());
  const [status, setStatus] = useState<SiteStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const sourceKey = JSON.stringify(saved.source);
  useFocusEffect(
    useCallback(() => {
      const end = activity.focus();
      const ticket = activity.begin()!;
      setBusy(true);
      setStatus(null);
      setError(null);
      void store.driver
        .request({ op: 'site_status', scope, source: saved.source })
        .then((result) => {
          if (ticket.current() && siteAuthority(store, scope) === authority) setStatus(result);
        })
        .catch((reason: unknown) => {
          if (ticket.current() && siteAuthority(store, scope) === authority)
            setError(reason instanceof Error ? reason.message : t('sites.publicationFailed'));
        })
        .finally(() => {
          if (ticket.finish()) setBusy(false);
        });
      return end;
      // Stable selected source, not every unrelated conversation snapshot.
      // oxlint-disable-next-line react/exhaustive-deps
    }, [activity, store, authority, sourceKey, revision]),
  );

  async function publish(unpublish: boolean) {
    if (busy || siteAuthority(store, scope) !== authority) return;
    const ticket = activity.begin();
    if (!ticket) return;
    setBusy(true);
    setError(null);
    try {
      const receipt = await publishObservedSite(
        store,
        scope,
        authority,
        saved.source,
        unpublish,
        ticket.current,
        (review, url) =>
          dialogs.confirm({
            title:
              review.mutation.change.kind === 'unpublish'
                ? t('sites.reviewUnpublish')
                : t('sites.reviewPublish'),
            message:
              review.mutation.change.kind === 'unpublish'
                ? t('sites.consentRemove', { url })
                : t('sites.consent', { version: review.source.version, url }),
            confirmLabel:
              review.mutation.change.kind === 'unpublish'
                ? t('sites.remove')
                : t('sites.confirmPublish'),
            cancelLabel: t('sites.cancel'),
            destructive: review.mutation.change.kind === 'unpublish',
          }),
      );
      if (receipt && ticket.current()) setRevision((value) => value + 1);
    } catch (reason) {
      if (ticket.current() && siteAuthority(store, scope) === authority) {
        setError(reason instanceof Error ? reason.message : t('sites.publicationFailed'));
        // Rediscover the same persisted intent; never mint a new retry locally.
        setRevision((value) => value + 1);
      }
    } finally {
      if (ticket.finish()) setBusy(false);
    }
  }
  if (siteAuthority(store, scope) !== authority) return null;
  const pending = status?.pending;
  const otherVersion = pending && !sameSource(pending.source, saved.source);
  return (
    <View style={{ gap: 6 }}>
      {status ? (
        <>
          {otherVersion ? (
            <NativeButton
              secondary
              label={t('sites.recoverVersion', { version: pending.source.version })}
              disabled={busy}
              onPress={() =>
                router.push({
                  pathname: '/site/[id]',
                  params: { id: pending.source.item_id, version: String(pending.source.version) },
                })
              }
            />
          ) : (
            <NativeButton
              secondary
              label={
                busy
                  ? t('sites.publishing')
                  : pending
                    ? t('sites.recover')
                    : t('sites.publish', { version: saved.source.version })
              }
              disabled={busy || (!pending && saved.html === null)}
              onPress={() => void publish(false)}
            />
          )}
          {status.inspection.active && !pending ? (
            <NativeButton
              secondary
              label={t('sites.remove')}
              disabled={busy}
              onPress={() => void publish(true)}
            />
          ) : null}
          {status.inspection.active ? (
            <NativeButton
              secondary
              label={t('sites.openPublic')}
              disabled={busy}
              onPress={() => {
                const url = `${status.configuration.public_origin}/s/${status.inspection.site_id}`;
                void Linking.openURL(url).catch(() => setError(t('sites.publicationFailed')));
              }}
            />
          ) : null}
        </>
      ) : null}
      {error ? (
        <>
          <Text accessibilityRole="alert" style={{ color: colors.danger }}>
            {error}
          </Text>
          <NativeButton
            secondary
            label={t('sites.retry')}
            disabled={busy}
            onPress={() => {
              setError(null);
              setRevision((value) => value + 1);
            }}
          />
        </>
      ) : null}
    </View>
  );
}
