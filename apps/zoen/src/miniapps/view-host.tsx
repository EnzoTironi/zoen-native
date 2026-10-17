import { t, locale } from '../i18n/index.ts';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import { Linking, Modal, ScrollView, Text, View } from 'react-native';
import * as Haptics from 'expo-haptics';
import type { ItemDetail } from '../core/types';
import type { AppSpecDto } from '../core/generated/AppSpecDto';
import type { AppResourceDto } from '../core/generated/AppResourceDto';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { NativeButton, Loading, Surface } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useTheme } from '../theme';
import { MiniappFrame } from './frame';
import type { MiniappFrameHandle } from './frame-types';
import { MiniappSession } from './protocol';
import {
  sandboxHtml,
  safeMiniappLink,
  verifyMiniappResource,
  type MiniappManifest,
} from './sandbox';
import { parseMiniappJson, stringifyMiniappRequest } from './json';
import { callMiniapp } from './actions';
import { photoArguments } from './photo-tokens';
import { available, perform, resolvePhoto, releaseItemPhotos } from './native-capabilities';
import { authorizedNetworkHosts } from './network';
import { frameAuthority } from './lifetime';

type Choice = 'once' | 'always' | 'deny';
type Consent = { capability: string; purpose: string; resolve: (choice: Choice) => void };
function useConsent() {
  const queue = useRef<Consent[]>([]);
  const [consent, setConsent] = useState<Consent | null>(null);
  const ask = useCallback(
    (capability: string, purpose: string) =>
      new Promise<Choice>((resolve) => {
        const next = { capability, purpose, resolve };
        queue.current.push(next);
        if (queue.current.length === 1) setConsent(next);
      }),
    [],
  );
  const finish = useCallback((choice: Choice) => {
    queue.current.shift()?.resolve(choice);
    setConsent(queue.current[0] ?? null);
  }, []);
  useEffect(
    () => () => {
      for (const request of queue.current.splice(0)) request.resolve('deny');
    },
    [],
  );
  return { consent, ask, finish };
}

export function MiniappView({ item, spec }: { item: ItemDetail; spec: AppSpecDto }) {
  const [focus, setFocus] = useState({ active: false, epoch: 0 });
  const end = useRef<(() => void) | null>(null);
  const registerClose = useCallback((close: (() => void) | null) => {
    end.current = close;
  }, []);
  useFocusEffect(
    useCallback(() => {
      setFocus((current) => ({ active: true, epoch: current.epoch + 1 }));
      return () => {
        // A covered stack screen can remain mounted/frozen. Close its document
        // and authority synchronously even before React commits the hidden state.
        end.current?.();
        setFocus((current) => ({ ...current, active: false }));
      };
    }, []),
  );
  return focus.active ? (
    <FocusedView key={focus.epoch} item={item} spec={spec} registerClose={registerClose} />
  ) : null;
}

function FocusedView({
  item,
  spec,
  registerClose,
}: {
  item: ItemDetail;
  spec: AppSpecDto;
  registerClose: (close: (() => void) | null) => void;
}) {
  const { store, snapshot, mode } = useClient();
  const { colors } = useTheme();
  const scope = `${mode}:${snapshot?.account?.identity_id ?? snapshot?.me?.id ?? ''}:${snapshot?.account?.device_id ?? ''}:${item.id}:${spec.resource_uri}`;
  const [read, setRead] = useState<{ scope: string; resource?: AppResourceDto; error?: string }>({
    scope,
  });
  const [revision, setRevision] = useState(0);
  const grants = useCoreQuery({ op: 'app_device_grants' });
  const [clock, setClock] = useState(Date.now);
  const [network, setNetwork] = useState<{ key: string; hosts: string[]; error?: string }>({
    key: '',
    hosts: [],
  });
  useEffect(() => {
    let active = true;
    void store.driver
      .request({ op: 'app_resource', uri: spec.resource_uri })
      .then((resource) => {
        if (active) setRead({ scope, resource });
      })
      .catch((reason: unknown) => {
        if (active)
          setRead({
            scope,
            error:
              reason instanceof Error ? reason.message : t('miniapps.theInterfaceCouldNotBeOpened'),
          });
      });
    return () => {
      active = false;
    };
    // Retrying reloads the core-bundled resource, never state or keys from an iframe.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [store, scope, spec.resource_uri, revision]);
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
  const prepared = useMemo(() => {
    if (read.scope !== scope || !read.resource) return null;
    try {
      if (read.resource.uri !== spec.resource_uri)
        throw new Error(t('miniapps.thisResourceDoesNotBelongToTheOpen'));
      return {
        resource: read.resource,
        manifest: verifyMiniappResource(read.resource, spec.id),
        error: null,
      };
    } catch (reason) {
      return {
        resource: read.resource,
        manifest: null,
        error:
          reason instanceof Error
            ? reason.message
            : t('miniapps.theMiniappPackageCouldNotBeVerified'),
      };
    }
  }, [read, scope, spec.id, spec.resource_uri]);
  const domainKey = JSON.stringify(prepared?.manifest?.allowedDomains ?? []);
  const grantKey = JSON.stringify(
    grants.data?.filter((grant) => grant.item_id === item.id) ?? null,
  );
  const authorizationKey = JSON.stringify([scope, domainKey, grantKey, clock, revision]);
  useEffect(() => {
    let active = true;
    setNetwork({ key: authorizationKey, hosts: [] });
    void authorizedNetworkHosts(
      JSON.parse(domainKey) as string[],
      (capability) =>
        store.driver.request({ op: 'app_device_allowed', item_id: item.id, capability }),
      () => {
        if (!active) throw new Error(t('miniapps.thisInterfaceHasClosed'));
      },
    )
      .then((hosts) => {
        if (active) setNetwork({ key: authorizationKey, hosts });
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
  }, [store, item.id, domainKey, authorizationKey]);
  if ((read.scope === scope && read.error) || prepared?.error)
    return (
      <Surface>
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
          {read.error ?? prepared?.error}
        </Text>
        <NativeButton
          secondary
          label={t('miniapps.checkAgain')}
          onPress={() => setRevision((value) => value + 1)}
        />
      </Surface>
    );
  if (!prepared) return <Loading />;
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
      resource={prepared.resource}
      manifest={prepared.manifest}
      networkHosts={networkHosts}
      networkError={network.key === authorizationKey ? network.error : undefined}
      onNetworkChange={grants.retry}
      registerClose={registerClose}
      onReload={() => setRevision((value) => value + 1)}
    />
  );
}

function ReadyView({
  item,
  spec,
  resource,
  manifest,
  networkHosts,
  networkError,
  onNetworkChange,
  registerClose,
  onReload,
}: {
  item: ItemDetail;
  spec: AppSpecDto;
  resource: AppResourceDto;
  manifest: MiniappManifest | null;
  networkHosts: string;
  networkError?: string;
  onNetworkChange: () => void;
  registerClose: (close: (() => void) | null) => void;
  onReload: () => void;
}) {
  const { store, snapshot, mode } = useClient();
  const { colors, scheme } = useTheme();
  const dialogs = useDialogs();
  const consent = useConsent();
  const ask = consent.ask;
  const hosts = useMemo(() => networkHosts.split(',').filter(Boolean), [networkHosts]);
  const [height, setHeight] = useState(680);
  const [width, setWidth] = useState(360);
  const [error, setError] = useState<string | null>(null);
  const frame = useRef<MiniappFrameHandle>(null);
  const session = useRef<MiniappSession | null>(null);
  const authority = useRef<ReturnType<typeof frameAuthority> | null>(null);
  const initialScope = `${mode}:${snapshot?.account?.identity_id ?? snapshot?.me?.id ?? ''}:${snapshot?.account?.device_id ?? ''}`;
  const resourceUri = spec.resource_uri;
  const appName = spec.name;
  const encodedTools = JSON.stringify(
    spec.tools.filter((tool) => tool.visibility.includes('app')).map((tool) => tool.name),
  );
  const tools = useMemo(() => new Set<string>(JSON.parse(encodedTools)), [encodedTools]);
  const latest = useRef({ item, snapshot, scheme, colors, width });
  useLayoutEffect(() => {
    latest.current = { item, snapshot, scheme, colors, width };
  }, [item, snapshot, scheme, colors, width]);
  const checkOwner = useCallback(() => {
    const state = store.getSnapshot();
    if (
      `${state.mode}:${state.snapshot?.account?.identity_id ?? state.snapshot?.me?.id ?? ''}:${state.snapshot?.account?.device_id ?? ''}` !==
      initialScope
    )
      throw new Error(t('miniapps.thisInterfaceHasClosedOrTheAccountHas'));
  }, [store, initialScope]);
  const permission = useCallback(
    async (capability: string, purpose: string, assertActive?: () => void) => {
      const check = assertActive ?? authority.current?.check;
      if (!check) throw new Error(t('miniapps.thisInterfaceHasClosed'));
      check();
      const allowed = await store.driver.request({
        op: 'app_device_allowed',
        item_id: item.id,
        capability,
      });
      check();
      if (allowed) return;
      const choice = await ask(capability, purpose);
      check();
      if (choice === 'deny') throw new Error(t('miniapps.youDidNotAllowThisAccess'));
      await store.request({
        op: 'grant_app_device',
        item_id: item.id,
        capability,
        purpose,
        always: choice === 'always',
      });
      check();
    },
    [store, item.id, ask],
  );
  const context = useCallback(() => {
    const current = latest.current;
    const members =
      current.snapshot?.conversations
        .find((chat) => chat.id === item.space_id)
        ?.members.filter((person) => person.kind === 'Person')
        .map((person) => ({
          id: person.id,
          name: person.name,
          initials: person.initials,
          color: person.tint_hex,
          isMe: person.is_me,
        })) ?? [];
    return {
      theme: current.scheme,
      styles: {
        variables: {
          '--color-background-primary': current.colors.background,
          '--color-background-secondary': current.colors.surface,
          '--color-text-primary': current.colors.text,
          '--color-text-secondary': current.colors.secondary,
          '--color-border-primary': current.colors.line,
          '--font-sans': 'system-ui, sans-serif',
        },
      },
      displayMode: 'fullscreen',
      availableDisplayModes: ['fullscreen', 'inline'],
      containerDimensions: { width: current.width },
      locale,
      timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      platform: process.env.EXPO_OS === 'web' ? 'web' : 'mobile',
      deviceCapabilities: { touch: true, hover: process.env.EXPO_OS === 'web' },
      safeAreaInsets: { top: 0, right: 0, bottom: 0, left: 0 },
      zoen: { members },
    };
  }, [item.space_id]);
  useLayoutEffect(() => {
    const lifetime = frameAuthority(checkOwner);
    authority.current = lifetime;
    const check = lifetime.check;
    const current = new MiniappSession({
      send: (message) => frame.current?.send(message),
      resourceUri,
      tools,
      initialize: () => ({
        protocolVersion: '2026-01-26',
        hostInfo: { name: 'zoen', version: '0.1.0' },
        hostCapabilities: {
          openLinks: {},
          serverTools: {},
          serverResources: {},
          sandbox: {
            csp: { connectDomains: hosts.map((host) => `https://${host}`) },
            permissions: {},
          },
          experimental: {
            zoen: {
              native: available().filter((capability) =>
                manifest?.capabilities.some((cap) => cap.id === capability),
              ),
              services: [
                'state',
                'members',
                'share',
                'theme',
                ...(process.env.EXPO_OS === 'web' ? [] : ['haptics']),
                ...hosts.map((host) => `net:${host}`),
              ],
              me: context().zoen.members.find((member) => member.isMe),
            },
          },
        },
        hostContext: context(),
      }),
      state: () => ({
        content: [{ type: 'text', text: latest.current.item.app?.headline ?? '' }],
        structuredContent: parseMiniappJson(latest.current.item.app?.view_json ?? '{}'),
        isError: false,
      }),
      context,
      resource: async () => {
        check();
        return {
          contents: [{ uri: resource.uri, mimeType: resource.mime_type, text: resource.text }],
        };
      },
      call: async (name, args) => {
        check();
        const photos = photoArguments(args, item.id, resolvePhoto);
        if (
          photos.tokens.length &&
          !(await dialogs.confirm({
            title: t('miniapps.sharePhotoCount', { count: photos.tokens.length }),
            message: t('miniapps.sharePhotoDetails', {
              chat: latest.current.item.space_title,
              app: manifest?.name ?? appName,
            }),
            confirmLabel: t('miniapps.shareWithGroup'),
          }))
        )
          throw new Error(t('miniapps.thePhotosWereNotShared'));
        check();
        const resolved = photos.tokens.length
          ? photoArguments(args, item.id, resolvePhoto, true).value
          : args;
        const result = await callMiniapp(
          store,
          dialogs,
          item.id,
          name,
          stringifyMiniappRequest(resolved),
          check,
        );
        check();
        if (!result) throw new Error(t('miniapps.theActionWasNotConfirmed'));
        return parseMiniappJson(result.result_json);
      },
      native: async (capability, params) => {
        check();
        const declared = manifest?.capabilities.find((cap) => cap.id === capability);
        if (!declared || !available().includes(capability))
          throw new Error(t('miniapps.thisFeatureIsNotDeclaredOrIsNot'));
        await permission(capability, declared.purpose, check);
        check();
        const result = await perform(capability, params, item.id);
        check();
        const stillAllowed = await store.driver.request({
          op: 'app_device_allowed',
          item_id: item.id,
          capability,
        });
        check();
        if (!stillAllowed) {
          releaseItemPhotos(item.id);
          throw new Error(t('miniapps.thisPermissionWasRevokedOrExpiredTheData'));
        }
        return result;
      },
      openLink: async (input) => {
        check();
        const url = safeMiniappLink(input, manifest);
        if (
          !(await dialogs.confirm({
            title: t('miniapps.openLinkOutsideZoen'),
            message: url,
            confirmLabel: t('miniapps.openLink'),
          }))
        )
          throw new Error(t('miniapps.youDidNotOpenTheLink'));
        check();
        await Linking.openURL(url);
        return {};
      },
      share: async (text) => {
        check();
        if (
          !(await dialogs.confirm({
            title: t('miniapps.sendInChat', { chat: latest.current.item.space_title }),
            message: text,
            confirmLabel: t('miniapps.sendMessage'),
          }))
        )
          throw new Error(t('miniapps.theMessageWasNotSent'));
        check();
        await store.request({
          op: 'send_message',
          space_id: item.space_id,
          text,
          reply_to: null,
          thread: false,
        });
        return {};
      },
      haptic: (kind) => {
        if (process.env.EXPO_OS === 'web') return;
        void (
          kind === 'success'
            ? Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success)
            : kind === 'warning'
              ? Haptics.notificationAsync(Haptics.NotificationFeedbackType.Warning)
              : Haptics.selectionAsync()
        ).catch(() => {});
      },
      resize: setHeight,
      close: () => router.back(),
    });
    session.current = current;
    const dispose = () => {
      lifetime.close();
      current.dispose();
      if (session.current === current) session.current = null;
      if (authority.current === lifetime) authority.current = null;
      releaseItemPhotos(item.id);
    };
    const close = () => {
      dispose();
      frame.current?.dispose();
    };
    registerClose(close);
    const stop = store.subscribe(() => {
      try {
        checkOwner();
      } catch {
        close();
      }
    });
    return () => {
      stop();
      dispose();
      registerClose(null);
    };
  }, [
    checkOwner,
    context,
    dialogs,
    hosts,
    item.id,
    item.space_id,
    manifest,
    permission,
    resource,
    appName,
    resourceUri,
    tools,
    registerClose,
    store,
  ]);
  useEffect(() => {
    session.current?.sync();
  }, [item.version, scheme, snapshot, width]);
  const html = useMemo(
    () => sandboxHtml(resource.text, hosts, process.env.EXPO_OS !== 'web'),
    [resource.text, hosts],
  );
  const receive = useCallback((message: string) => {
    void session.current?.receive(message);
  }, []);
  return (
    <View onLayout={(event) => setWidth(event.nativeEvent.layout.width)} style={{ gap: 12 }}>
      {error || networkError ? (
        <View style={{ gap: 8 }}>
          <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
            {error ?? networkError}
          </Text>
          <NativeButton secondary label={t('miniapps.reopenInterface')} onPress={onReload} />
        </View>
      ) : null}
      {manifest?.allowedDomains.some((host) => !hosts.includes(host)) ? (
        <NativeButton
          secondary
          label={t('miniapps.enableOnlineMaps')}
          onPress={() => {
            const check = authority.current?.check;
            if (!check) return;
            void (async () => {
              for (const host of manifest.allowedDomains) {
                await permission(
                  `net:${host}`,
                  manifest.networkPurpose || t('miniapps.loadContentFromThisDomain'),
                  check,
                );
              }
              check();
              onNetworkChange();
            })().catch((reason: unknown) =>
              setError(
                reason instanceof Error
                  ? reason.message
                  : t('miniapps.theConnectionCouldNotBeAllowed'),
              ),
            );
          }}
        />
      ) : null}
      <MiniappFrame
        ref={frame}
        html={html}
        title={item.title}
        height={height}
        blocked={!!consent.consent}
        onMessage={receive}
        onError={setError}
      />
      <Modal
        visible={!!consent.consent}
        transparent
        animationType="fade"
        onRequestClose={() => consent.finish('deny')}
      >
        <View
          accessibilityViewIsModal
          style={{ flex: 1, backgroundColor: '#0007', justifyContent: 'center', padding: 24 }}
        >
          <ScrollView
            contentContainerStyle={{
              maxWidth: 460,
              width: '100%',
              alignSelf: 'center',
              flexGrow: 1,
              justifyContent: 'center',
            }}
          >
            <Surface>
              <Text
                accessibilityRole="header"
                style={{ color: colors.text, fontSize: 23, fontWeight: '600' }}
              >
                {t('miniapps.allowAccessToApp', { app: manifest?.name ?? spec.name })}
              </Text>
              <Text selectable style={{ color: colors.secondary, lineHeight: 23 }}>
                {consent.consent?.purpose}
              </Text>
              <Text style={{ color: colors.secondary, fontSize: 13, lineHeight: 20 }}>
                {t('miniapps.accessScope', { chat: item.space_title })}
              </Text>
              <NativeButton
                label={t('miniapps.allowOnce')}
                onPress={() => consent.finish('once')}
              />
              <NativeButton
                secondary
                label={t('miniapps.allowUntilIRevoke')}
                onPress={() => consent.finish('always')}
              />
              <NativeButton
                secondary
                label={t('miniapps.doNotAllow')}
                onPress={() => consent.finish('deny')}
              />
            </Surface>
          </ScrollView>
        </View>
      </Modal>
    </View>
  );
}
