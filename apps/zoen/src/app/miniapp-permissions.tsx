import { t } from '../i18n/index.ts';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import { createNetworkConsent, type NetworkConsentView } from '../miniapps/network-consent';
import { useDeviceConsent } from '../miniapps/use-device-consent';
import { DeviceConsent } from '../miniapps/device-consent';
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
  const consent = useDeviceConsent();
  const [networkContext, setNetworkContext] = useState<{ app: string; chat: string } | null>(null);
  const read = useCoreQuery({ op: 'app_device_grants' });
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const grants = read.data?.filter((grant) => !item || grant.item_id === item);
  const [grantClock, setGrantClock] = useState(Date.now);
  useEffect(() => {
    const expiries =
      read.data
        ?.filter((grant) => (!item || grant.item_id === item) && !grant.always)
        .map((grant) => Number(BigInt(grant.at_ms) + 60_000n))
        .filter((at) => at > Date.now()) ?? [];
    if (!expiries.length) return;
    const timer = setTimeout(
      () => {
        setGrantClock(Date.now());
        read.retry();
      },
      Math.max(1, Math.min(...expiries) - Date.now() + 1),
    );
    return () => clearTimeout(timer);
  }, [read, item]);
  const grantRevision = JSON.stringify([grants, grantClock]);
  return (
    <View style={{ flex: 1 }}>
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
        {item ? (
          <ItemNetworkAccess
            itemId={item}
            grantRevision={grantRevision}
            onChange={read.retry}
            consent={consent}
            onContext={setNetworkContext}
          />
        ) : null}
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
      <DeviceConsent
        request={consent.current}
        phase={consent.phase}
        title={t('miniapps.allowAccessToApp', { app: networkContext?.app ?? '' })}
        scope={t('miniapps.accessScope', { chat: networkContext?.chat ?? '' })}
        onChoose={consent.choose}
        onDismiss={consent.dismissed}
      />
    </View>
  );
}

function ItemNetworkAccess({
  itemId,
  grantRevision,
  onChange,
  consent,
  onContext,
}: {
  itemId: string;
  grantRevision: string;
  onChange: () => void;
  consent: ReturnType<typeof useDeviceConsent>;
  onContext: (value: { app: string; chat: string } | null) => void;
}) {
  const { store, snapshot, mode } = useClient();
  const { colors } = useTheme();
  const { ask, cancelAll } = consent;
  const owner = `${mode}:${store.sessionEpoch}:${snapshot?.account?.identity_id ?? snapshot?.me?.id ?? ''}:${snapshot?.account?.device_id ?? ''}`;
  const current = useRef<ReturnType<typeof createNetworkConsent> | null>(null);
  const working = useRef<ReturnType<typeof createNetworkConsent> | null>(null);
  const changed = useRef(onChange);
  useLayoutEffect(() => {
    changed.current = onChange;
  }, [onChange]);
  const [revision, setRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  const [heldGrantRevision, setHeldGrantRevision] = useState(grantRevision);
  const focusKey = JSON.stringify([
    itemId,
    owner,
    busy ? heldGrantRevision : grantRevision,
    revision,
  ]);
  const [network, setNetwork] = useState<{ key: string; policy: NetworkConsentView | null }>({
    key: '',
    policy: null,
  });
  const policy = network.key === focusKey ? network.policy : null;
  const [error, setError] = useState<string | null>(null);
  useFocusEffect(
    useCallback(() => {
      const scope = createNetworkConsent(itemId, {
        store,
        read: (request) => store.driver.request(request),
        write: (request) => store.request(request),
        ask,
        cancelConsent: cancelAll,
      });
      current.current = scope;
      setNetwork({ key: focusKey, policy: null });
      onContext(null);
      setError(null);
      setBusy(false);
      void scope
        .read()
        .then((value) => {
          scope.check();
          if (current.current === scope) {
            setNetwork({ key: focusKey, policy: value });
            onContext(value ? { app: value.manifest.name, chat: value.spaceTitle } : null);
          }
        })
        .catch((reason: unknown) => {
          if (current.current === scope)
            setError(
              reason instanceof Error
                ? reason.message
                : t('miniapps.networkAccessCouldNotBeVerified'),
            );
        });
      return () => {
        if (current.current === scope) current.current = null;
        scope.close();
      };
    }, [store, itemId, focusKey, ask, cancelAll, onContext]),
  );
  if (error)
    return (
      <Surface>
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
        <NativeButton
          secondary
          label={t('miniapps.checkAgain')}
          onPress={() => setRevision((value) => value + 1)}
        />
      </Surface>
    );
  if (!policy?.manifest.allowedDomains.some((host) => !policy.allowedHosts.includes(host)))
    return null;
  return (
    <Surface>
      <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>
        {policy.itemTitle}
      </Text>
      <Text style={{ color: colors.secondary }}>
        {policy.manifest.networkPurpose || t('miniapps.loadContentFromThisDomain')}
      </Text>
      {policy.manifest.allowedDomains.map((host) => (
        <Text key={host} style={{ color: colors.secondary }}>
          {t('miniapps.networkAccess', { host })}
        </Text>
      ))}
      <NativeButton
        secondary
        label={t('miniapps.enableOnlineMaps')}
        disabled={busy}
        onPress={() => {
          const scope = current.current;
          if (!scope || working.current === scope) return;
          working.current = scope;
          setHeldGrantRevision(grantRevision);
          setBusy(true);
          let accepted = false;
          void scope
            .grant()
            .then(() => {
              scope.check();
              if (current.current !== scope) return;
              accepted = true;
              changed.current();
            })
            .catch((reason: unknown) => {
              if (current.current === scope)
                setError(
                  reason instanceof Error
                    ? reason.message
                    : t('miniapps.theConnectionCouldNotBeAllowed'),
                );
            })
            .finally(() => {
              if (working.current === scope) working.current = null;
              if (current.current === scope) {
                setBusy(false);
                if (accepted) setRevision((value) => value + 1);
              }
            });
        }}
      />
    </Surface>
  );
}
