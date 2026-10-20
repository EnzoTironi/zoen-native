import { useAppSheetCloseButton } from './app-sheet-context';
import { t, locale } from '../i18n/index.ts';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import { Linking, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import * as Haptics from 'expo-haptics';
import type { ItemDetail } from '../core/types';
import type { AppSpecDto } from '../core/generated/AppSpecDto';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { NativeButton, Loading, Surface } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useTheme } from '../theme';
import { MiniappFrame } from './frame';
import type { MiniappFrameHandle } from './frame-types';
import { MiniappSession } from './protocol';
import { sandboxHtml, safeMiniappLink, type VerifiedMiniappResource } from './sandbox';
import { readVerifiedMiniappResource } from './resource-load';
import { parseMiniappJson, stringifyMiniappRequest } from './json';
import { callMiniapp } from './actions';
import { photoArguments } from './photo-tokens';
import { available, perform, resolvePhoto, releaseItemPhotos } from './native-capabilities';
import { authorizedNetworkHosts } from './network';
import { frameAuthority, frameClosures } from './lifetime';
import { DeviceConsent } from './device-consent';
import { useDeviceConsent } from './use-device-consent';
import { CanonicalDrawingView } from './canonical-view';
import { DeckcraftView } from './deckcraft-view';
import type { MiniappDocumentController } from './document-controller';

export function MiniappView({
  onDocumentController,
  onDocumentCopied,
  onDocumentDiscard,
  ...props
}: {
  item: ItemDetail;
  spec: AppSpecDto;
  fill?: boolean;
  onDocumentController?: (controller: MiniappDocumentController | null) => void;
  onDocumentCopied?: (itemId: string) => void;
  onDocumentDiscard?: () => void;
}) {
  return props.spec.id === 'deckcraft' ? (
    <DeckcraftView
      {...props}
      onDocumentController={onDocumentController}
      onDocumentDiscard={onDocumentDiscard}
    />
  ) : props.spec.id === 'excalidraw' ? (
    <CanonicalDrawingView
      {...props}
      onDocumentController={onDocumentController}
      onDocumentCopied={onDocumentCopied}
      onDocumentDiscard={onDocumentDiscard}
    />
  ) : (
    <BuiltinMiniappView {...props} />
  );
}

function BuiltinMiniappView({
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

function ReadyView({
  item,
  spec,
  fill,
  verified,
  checkOwner,
  networkHosts,
  networkError,
  onNetworkChange,
  registerClose,
  onReload,
}: {
  item: ItemDetail;
  spec: AppSpecDto;
  fill: boolean;
  verified: VerifiedMiniappResource;
  checkOwner: () => void;
  networkHosts: string;
  networkError?: string;
  onNetworkChange: () => void;
  registerClose: (close: () => void) => () => void;
  onReload: () => void;
}) {
  const { resource, manifest } = verified;
  const { store, snapshot } = useClient();
  const { colors, scheme } = useTheme();
  const insets = useSafeAreaInsets();
  const closeButton = useAppSheetCloseButton();
  const safeAreaInsets = useMemo(
    () => (fill ? insets : { top: 0, right: 0, bottom: 0, left: 0 }),
    [fill, insets],
  );
  const dialogs = useDialogs();
  const consent = useDeviceConsent();
  const ask = consent.ask;
  const cancelConsent = consent.cancelAll;
  const hosts = useMemo(() => networkHosts.split(',').filter(Boolean), [networkHosts]);
  const [height, setHeight] = useState(680);
  const [dimensions, setDimensions] = useState({ width: 360, height: 680 });
  const [error, setError] = useState<string | null>(null);
  const frame = useRef<MiniappFrameHandle>(null);
  const session = useRef<MiniappSession | null>(null);
  const authority = useRef<ReturnType<typeof frameAuthority> | null>(null);
  const resourceUri = spec.resource_uri;
  const encodedTools = JSON.stringify(
    spec.tools.filter((tool) => tool.visibility.includes('app')).map((tool) => tool.name),
  );
  const tools = useMemo(() => new Set<string>(JSON.parse(encodedTools)), [encodedTools]);
  const latest = useRef({
    item,
    snapshot,
    scheme,
    colors,
    dimensions,
    safeAreaInsets,
    closeButton,
  });
  useLayoutEffect(() => {
    latest.current = { item, snapshot, scheme, colors, dimensions, safeAreaInsets, closeButton };
  }, [item, snapshot, scheme, colors, dimensions, safeAreaInsets, closeButton]);
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
      containerDimensions: current.dimensions,
      locale,
      timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      platform: process.env.EXPO_OS === 'web' ? 'web' : 'mobile',
      deviceCapabilities: { touch: true, hover: process.env.EXPO_OS === 'web' },
      safeAreaInsets: current.safeAreaInsets,
      zoen: { members, chrome: { closeButton: fill ? current.closeButton : null } },
    };
  }, [item.space_id, fill]);
  useLayoutEffect(() => {
    const lifetime = frameAuthority(checkOwner);
    try {
      lifetime.check();
    } catch {
      lifetime.close();
      frame.current?.dispose();
      return;
    }
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
          message: { text: {} },
          serverTools: {},
          serverResources: {},
          sandbox: {
            csp: { connectDomains: hosts.map((host) => `https://${host}`) },
            permissions: {},
          },
          experimental: {
            zoen: {
              native: available().filter((capability) =>
                manifest.capabilities.some((cap) => cap.id === capability),
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
              app: manifest.name,
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
        const declared = manifest.capabilities.find((cap) => cap.id === capability);
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
      resize: (nextHeight) => {
        // Native fullscreen owns the viewport; HTML content must not grow an outer scroll view.
        if (!fill) setHeight(nextHeight);
      },
      close: () => router.back(),
    });
    session.current = current;
    const dispose = () => {
      lifetime.close();
      cancelConsent();
      current.dispose();
      if (session.current === current) session.current = null;
      if (authority.current === lifetime) authority.current = null;
      releaseItemPhotos(item.id);
    };
    const close = () => {
      dispose();
      frame.current?.dispose();
    };
    const unregisterClose = registerClose(close);
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
      unregisterClose();
    };
  }, [
    cancelConsent,
    checkOwner,
    context,
    fill,
    dialogs,
    hosts,
    item.id,
    item.space_id,
    manifest,
    permission,
    resource,
    resourceUri,
    tools,
    registerClose,
    store,
  ]);
  useEffect(() => {
    session.current?.sync();
  }, [item.version, snapshot]);
  useEffect(() => {
    session.current?.syncContext();
  }, [scheme, dimensions, safeAreaInsets, closeButton]);
  const html = useMemo(
    () => sandboxHtml(resource.text, hosts, manifest.runtimePackage),
    [resource.text, hosts, manifest.runtimePackage],
  );
  const receive = useCallback((message: string) => {
    void session.current?.receive(message);
  }, []);
  return (
    <View style={fill ? { flex: 1, minHeight: 0, gap: 12 } : { gap: 12 }}>
      {error || networkError ? (
        <View style={{ gap: 8 }}>
          <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
            {error ?? networkError}
          </Text>
          <NativeButton secondary label={t('miniapps.reopenInterface')} onPress={onReload} />
        </View>
      ) : null}
      {!fill && manifest.allowedDomains.some((host) => !hosts.includes(host)) ? (
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
      <View
        style={fill ? { flex: 1, minHeight: 0 } : undefined}
        onLayout={(event) => {
          const { width, height } = event.nativeEvent.layout;
          if (width > 0 && height > 0)
            setDimensions((previous) =>
              previous.width === width && previous.height === height ? previous : { width, height },
            );
        }}
      >
        <MiniappFrame
          ref={frame}
          html={html}
          runtimePackage={manifest.runtimePackage}
          title={item.title}
          height={height}
          fill={fill}
          blocked={!!consent.current}
          onMessage={receive}
          onError={setError}
        />
      </View>
      <DeviceConsent
        request={consent.current}
        phase={consent.phase}
        title={t('miniapps.allowAccessToApp', { app: manifest.name })}
        scope={t('miniapps.accessScope', { chat: item.space_title })}
        onChoose={consent.choose}
        onDismiss={consent.dismissed}
      />
    </View>
  );
}
