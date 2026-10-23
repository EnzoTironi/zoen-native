import { useState } from 'react';
import { Stack } from 'expo-router';
import { EmptyState, Loading, NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import type { ItemDetail } from '../core/types';
import type { SiteScope } from '../core/generated/SiteScope';
import { t } from '../i18n/index.ts';
import { siteAuthority, siteScope } from './source';
import { SiteEditor } from './editor';

/** Pin the selected saved version for this presentation. New remote versions
 * never replace an unsaved local source editor underneath someone's hands. */
export function SiteDocument({ itemId, version }: { itemId: string; version?: number }) {
  const { store } = useClient();
  const read = useCoreQuery(
    version ? { op: 'item_at', item_id: itemId, version } : { op: 'item', item_id: itemId },
  );
  const item = read.data;
  const scope = item ? siteScope(store, item.space_id) : null;
  const authority = scope ? siteAuthority(store, scope) : null;
  if (!item || !scope || !authority || item.kind_id !== 'file' || item.file?.mime !== 'text/html')
    return read.loading ? (
      <Loading />
    ) : (
      <EmptyState title={t('sites.unavailable')} detail={read.error ?? undefined} />
    );
  return (
    <PinnedSite
      key={`${authority}:${itemId}`}
      item={item}
      expectedSha={item.file.sha256}
      scope={scope}
      authority={authority}
    />
  );
}

function PinnedSite({
  item,
  expectedSha,
  scope,
  authority,
}: {
  item: ItemDetail;
  expectedSha: string;
  scope: SiteScope;
  authority: string;
}) {
  const [selected] = useState(() => ({
    item_id: item.id,
    version: item.version,
    expected_sha256: expectedSha,
  }));
  const read = useCoreQuery({ op: 'site_read', scope, ...selected });
  const { store } = useClient();
  if (siteAuthority(store, scope) !== authority)
    return <EmptyState title={t('sites.unavailable')} />;
  if (!read.data)
    return read.error ? (
      <EmptyState title={t('sites.unavailable')} detail={read.error}>
        <NativeButton label={t('sites.retry')} onPress={read.retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  return (
    <>
      <Stack.Screen options={{ title: read.data.name }} />
      <SiteEditor
        initial={read.data}
        scope={scope}
        authority={authority}
        available={!read.error}
        refresh={read.retry}
      />
    </>
  );
}
