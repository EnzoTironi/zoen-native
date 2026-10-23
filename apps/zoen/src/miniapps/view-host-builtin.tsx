import { useFocusEffect } from 'expo-router';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Text } from 'react-native';
import { Loading, NativeButton, Surface } from '../components/primitives';
import type { AppSpecDto } from '../core/generated/AppSpecDto';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import type { ItemDetail } from '../core/types';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { frameAuthority, frameClosures } from './lifetime';
import { authorizedNetworkHosts } from './network';
import { readVerifiedMiniappResource } from './resource-load';
import { type VerifiedMiniappResource } from './sandbox';
import { ReadyView } from './view-host-ready';

export function BuiltinMiniappView({
  item,
  spec,
  fill = false,
}: {
  item: ItemDetail;
  spec: AppSpecDto;
  fill?: boolean;
}) {
  const [focus, setFocus] = useState<{
    epoch: number;
    lifetime: ReturnType<typeof frameClosures> | null;
  }>({ epoch: 0, lifetime: null });
  useFocusEffect(
    useCallback(() => {
      const lifetime = frameClosures();
      setFocus((current) => ({ lifetime, epoch: current.epoch + 1 }));
      return () => {
        try {
          lifetime.close();
        } finally {
          setFocus((current) =>
            current.lifetime === lifetime ? { ...current, lifetime: null } : current,
          );
        }
      };
    }, []),
  );
  return focus.lifetime ? (
    <FocusedView
      key={focus.epoch}
      item={item}
      spec={spec}
      fill={fill}
      registerClose={focus.lifetime.register}
    />
  ) : null;
}

function FocusedView({
  item,
  spec,
  fill,
  registerClose,
}: {
  item: ItemDetail;
  spec: AppSpecDto;
  fill: boolean;
  registerClose: (close: () => void) => () => void;
}) {
  const { store, snapshot, mode } = useClient();
  const { colors } = useTheme();
  const scope = `${mode}:${snapshot?.account?.identity_id ?? snapshot?.me?.id ?? ''}:${snapshot?.account?.device_id ?? ''}:${item.id}:${spec.resource_uri}`;
  const epoch = useRef(store.sessionEpoch).current;
  const [revision, setRevision] = useState(0);
  const readKey = JSON.stringify([scope, spec.id, revision]);
  const [read, setRead] = useState<{
    key?: string;
    verified?: VerifiedMiniappResource;
    check?: () => void;
    error?: string;
  }>({});
  const checkOwner = useCallback(() => {
    const state = store.getSnapshot();
    const account = state.snapshot?.account;
    const current = `${state.mode}:${account?.identity_id ?? state.snapshot?.me?.id ?? ''}:${account?.device_id ?? ''}:${item.id}:${spec.resource_uri}`;
    if (
      store.sessionEpoch !== epoch ||
      state.phase !== 'ready' ||
      state.erasePending ||
      (state.mode === 'account' && (!account?.unlocked || account.setup_pending)) ||
      current !== scope ||
      !state.snapshot?.conversations.some((chat) => chat.id === item.space_id)
    )
      throw new Error(t('miniapps.thisInterfaceHasClosedOrTheAccountHas'));
  }, [store, epoch, scope, item.id, item.space_id, spec.resource_uri]);
  const grants = useCoreQuery({ op: 'app_device_grants' });
  const [clock, setClock] = useState(Date.now);
  const [network, setNetwork] = useState<{ key: string; hosts: string[]; error?: string }>({
    key: '',
    hosts: [],
  });
  useEffect(() => {
    let active = true;
    const lifetime = frameAuthority(checkOwner);
    const unregisterClose = registerClose(() => {
      active = false;
      lifetime.close();
    });
    void readVerifiedMiniappResource(
      () => store.driver.request({ op: 'app_resource', uri: spec.resource_uri }),
      spec.id,
      spec.resource_uri,
      lifetime.check,
    )
      .then((verified) => {
        if (!active) return;
        lifetime.check();
        setRead({ key: readKey, verified, check: lifetime.check });
      })
      .catch((reason: unknown) => {
        if (active)
          setRead({
            key: readKey,
            error:
              reason instanceof Error ? reason.message : t('miniapps.theInterfaceCouldNotBeOpened'),
          });
      });
    return () => {
      active = false;
      lifetime.close();
      unregisterClose();
    };
  }, [store, registerClose, checkOwner, readKey, spec.id, spec.resource_uri]);
  useEffect(() => {
    const expiries =
      grants.data
        ?.filter((grant) => grant.item_id === item.id && !grant.always)
        .map((grant) => Number(BigInt(grant.at_ms) + 60_000n))
        .filter((at) => at > Date.now()) ?? [];
    if (!expiries.length) return;
    const timer = setTimeout(
      () => {
        setClock(Date.now());
        grants.retry();
      },
      Math.max(1, Math.min(...expiries) - Date.now() + 1),
    );
    return () => clearTimeout(timer);
  }, [grants, item.id]);
  const prepared = read.key === readKey ? read.verified : undefined;
  const domainKey = JSON.stringify(prepared?.manifest?.allowedDomains ?? []);
  const grantKey = JSON.stringify(
    grants.data?.filter((grant) => grant.item_id === item.id) ?? null,
  );
  const authorizationKey = JSON.stringify([scope, domainKey, grantKey, clock, revision]);
  useEffect(() => {
    let active = true;
    void authorizedNetworkHosts(
      JSON.parse(domainKey) as string[],
      (capability) =>
        store.driver.request({ op: 'app_device_allowed', item_id: item.id, capability }),
      () => {
        if (!active) throw new Error(t('miniapps.thisInterfaceHasClosed'));
        checkOwner();
      },
    )
      .then((hosts) => {
        if (!active) return;
        checkOwner();
        setNetwork({ key: authorizationKey, hosts });
      })
      .catch((reason: unknown) => {
        if (active)
          setNetwork({
            key: authorizationKey,
            hosts: [],
            error:
              reason instanceof Error
                ? reason.message
                : t('miniapps.networkAccessCouldNotBeVerified'),
          });
      });
    return () => {
      active = false;
    };
  }, [store, item.id, domainKey, authorizationKey, checkOwner]);
  if (read.key === readKey && read.error)
    return (
      <Surface>
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
          {read.error}
        </Text>
        <NativeButton
          secondary
          label={t('miniapps.checkAgain')}
          onPress={() => setRevision((value) => value + 1)}
        />
      </Surface>
    );
  if (!prepared || !read.check) return <Loading />;
  const networkHosts = network.key === authorizationKey ? network.hosts.join(',') : '';
  const configuration = JSON.stringify({
    name: spec.name,
    tools: spec.tools.filter((tool) => tool.visibility.includes('app')).map((tool) => tool.name),
  });
  return (
    <ReadyView
      key={`${scope}:${revision}:${networkHosts}:${configuration}`}
      item={item}
      spec={spec}
      fill={fill}
      verified={prepared}
      checkOwner={read.check}
      networkHosts={networkHosts}
      networkError={network.key === authorizationKey ? network.error : undefined}
      onNetworkChange={grants.retry}
      registerClose={registerClose}
      onReload={() => setRevision((value) => value + 1)}
    />
  );
}
