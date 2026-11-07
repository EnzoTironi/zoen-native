import { useEffect, useState } from 'react';
import { Host, TextInput } from '@expo/ui';
import { Alert, Text, View } from 'react-native';
import { NativeButton, Loading } from '../components/primitives';
import { useTheme } from '../theme';
import { t, formatLocale } from '../i18n/index.ts';
import { messageInfoDate } from '../core/message-info-presentation';
import type { SpaceInviteRecord } from '../core/generated/SpaceInviteRecord';
import type { useSpaceInvite } from './use-space-invite';

type Controller = ReturnType<typeof useSpaceInvite>;
export function SpaceInviteManagement({ invite }: { invite: Controller }) {
  const { presentation: p } = useTheme();
  const page = invite.management;
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const future =
      page?.entries.map((entry) => Number(entry.expires_at_ms)).filter((expiry) => expiry > now) ??
      [];
    if (!future.length) return;
    const timer = setTimeout(
      () => setNow(Date.now()),
      Math.min(2147483647, Math.max(1, Math.min(...future) - Date.now())),
    );
    return () => clearTimeout(timer);
  }, [page, now]);
  return (
    <View style={{ gap: 16 }}>
      <NativeButton
        testID="space-invite-configure"
        label={t('spaceInvite.newLink')}
        onPress={invite.configure}
        disabled={invite.busy}
      />
      {invite.busy ? <Loading /> : null}
      {page?.indexing ? (
        <Text style={{ color: p.list.secondary }}>{t('spaceInvite.indexing')}</Text>
      ) : null}
      {page && !page.indexing && !page.entries.length ? (
        <Text style={{ color: p.list.secondary }}>{t('spaceInvite.empty')}</Text>
      ) : null}
      {page?.entries.map((record) => (
        <InviteRow key={record.id} record={record} invite={invite} now={now} />
      ))}
      {page?.next ? (
        <NativeButton
          testID="space-invite-next"
          secondary
          label={t('spaceInvite.next')}
          disabled={invite.busy}
          onPress={() => invite.manage({ kind: 'list', after: page.next })}
        />
      ) : null}
      <NativeButton
        testID="space-invite-refresh"
        secondary
        label={t('spaceInvite.refresh')}
        disabled={invite.busy}
        onPress={() => invite.manage({ kind: 'list', after: '' })}
      />
    </View>
  );
}
function InviteRow({
  record,
  invite,
  now,
}: {
  record: SpaceInviteRecord;
  invite: Controller;
  now: number;
}) {
  const { presentation: p, scheme } = useTheme();
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(record.name);
  const manageable = invite.isOwner || !['Owner', 'Admin'].includes(record.role);
  const expired = BigInt(record.expires_at_ms) <= BigInt(now);
  const status = record.revoked
    ? t('spaceInvite.revoked')
    : expired
      ? t('spaceInvite.expired')
      : record.uses >= record.max_uses
        ? t('spaceInvite.exhausted')
        : null;
  return (
    <View
      style={{
        gap: 10,
        paddingVertical: 12,
        borderBottomWidth: 0.5,
        borderBottomColor: p.list.secondary,
      }}
    >
      <Text style={{ color: p.list.text, fontSize: 17, fontWeight: '600' }}>
        {record.name || t('spaceInvite.unnamed')}
      </Text>
      <Text style={{ color: p.list.secondary }}>
        {t('spaceInvite.used', { used: record.uses, limit: record.max_uses })}
      </Text>
      <Text style={{ color: p.list.secondary }}>
        {status ??
          t('spaceInvite.expires', {
            date:
              messageInfoDate(record.expires_at_ms, formatLocale) ??
              t('messageInfo.timeUnavailable'),
          })}
      </Text>
      {manageable && editing ? (
        <>
          <Host matchContents={{ vertical: true }} colorScheme={scheme} style={{ width: '100%' }}>
            <TextInput
              testID={`space-invite-name-${record.id}`}
              defaultValue={record.name}
              maxLength={64}
              placeholder={t('spaceInvite.name')}
              onChangeText={setName}
              editable={!invite.busy}
              autoFocus
            />
          </Host>
          <NativeButton
            testID={`space-invite-save-${record.id}`}
            label={t('profile.save')}
            disabled={invite.busy}
            onPress={() => {
              invite.manage({ kind: 'rename', id: record.id, name }, () => setEditing(false));
            }}
          />
          <NativeButton
            secondary
            label={t('common.cancel')}
            disabled={invite.busy}
            onPress={() => setEditing(false)}
          />
        </>
      ) : manageable ? (
        <NativeButton
          testID={`space-invite-rename-${record.id}`}
          secondary
          label={t('spaceInvite.rename')}
          disabled={invite.busy}
          onPress={() => {
            setName(record.name);
            setEditing(true);
          }}
        />
      ) : null}
      {manageable && !record.revoked ? (
        <NativeButton
          testID={`space-invite-revoke-${record.id}`}
          secondary
          label={t('spaceInvite.revoke')}
          disabled={invite.busy}
          onPress={() =>
            Alert.alert(t('spaceInvite.revoke'), t('spaceInvite.revokeConfirm'), [
              { text: t('common.cancel'), style: 'cancel' },
              {
                text: t('spaceInvite.revoke'),
                style: 'destructive',
                onPress: () => invite.manage({ kind: 'revoke', id: record.id }),
              },
            ])
          }
        />
      ) : null}
    </View>
  );
}
