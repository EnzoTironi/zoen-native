import { t } from '../i18n/index.ts';
import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text } from 'react-native';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { EmptyState, Loading, NativeButton, Surface } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useTheme } from '../theme';

const labels: Record<string, string> = {
  'photos.pick': t('miniapps.choosePhotos'),
  location: t('miniapps.location'),
  'calendar.freebusy': t('miniapps.calendarAvailability'),
  'calendar.write': t('miniapps.createCalendarEvents'),
  camera: t('miniapps.camera'),
};
export default function MiniappPermissions() {
  const { item } = useLocalSearchParams<{ item?: string }>();
  const { store } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const read = useCoreQuery({ op: 'app_device_grants' });
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const grants = read.data?.filter((grant) => !item || grant.item_id === item);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 20,
        maxWidth: 800,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: t('miniapps.miniappPermissions') }} />
      <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 24 }}>
        {t('miniapps.eachPermissionBelongsToAMiniappInOne')}
      </Text>
      {error || read.error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error ?? read.error}
        </Text>
      ) : null}
      {!grants ? (
        read.error ? (
          <NativeButton label={t('miniapps.tryAgain')} onPress={read.retry} />
        ) : (
          <Loading />
        )
      ) : !grants.length ? (
        <EmptyState
          title={t('miniapps.noActivePermissions')}
          detail={t('miniapps.whenYouAllowAccessToAFeatureThe')}
        />
      ) : (
        grants.map((grant) => (
          <Surface key={grant.grant_id}>
            <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>
              {grant.item_title}
            </Text>
            <Text style={{ color: colors.accent }}>
              {labels[grant.capability] ??
                (grant.capability.startsWith('net:')
                  ? t('miniapps.networkAccess', { host: grant.capability.slice(4) })
                  : grant.capability)}
            </Text>
            <Text selectable style={{ color: colors.secondary, lineHeight: 22 }}>
              {grant.purpose}
            </Text>
            <Text style={{ color: colors.secondary, fontSize: 13 }}>
              {grant.space_title} ·{' '}
              {grant.always ? t('miniapps.untilYouRevoke') : t('miniapps.temporaryPermission')}
            </Text>
            <NativeButton
              secondary
              label={t('miniapps.openMiniapp')}
              disabled={busy !== null}
              onPress={() =>
                router.push({ pathname: '/miniapp/[id]', params: { id: grant.item_id } })
              }
            />
            <NativeButton
              secondary
              label={
                busy === grant.grant_id ? t('miniapps.revoking') : t('miniapps.revokePermission')
              }
              disabled={busy !== null}
              onPress={() => {
                void dialogs
                  .confirm({
                    title: t('miniapps.revokeThisPermission'),
                    message: t('miniapps.revokePermissionDetails', {
                      title: grant.item_title,
                      capability: labels[grant.capability] ?? grant.capability,
                    }),
                    confirmLabel: t('miniapps.revoke'),
                    destructive: true,
                  })
                  .then((accepted) => {
                    if (!accepted) return;
                    setBusy(grant.grant_id);
                    setError(null);
                    void store
                      .request({ op: 'revoke_app_device', grant_id: grant.grant_id })
                      .then(read.retry)
                      .catch((reason: unknown) =>
                        setError(
                          reason instanceof Error
                            ? reason.message
                            : t('miniapps.thePermissionCouldNotBeRevoked'),
                        ),
                      )
                      .finally(() => setBusy(null));
                  });
              }}
            />
          </Surface>
        ))
      )}
    </ScrollView>
  );
}
