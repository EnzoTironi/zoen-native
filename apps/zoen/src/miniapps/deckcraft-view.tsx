import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { AppState, Text, View } from 'react-native';
import { router, useFocusEffect } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import * as Crypto from 'expo-crypto';
import { locale, t } from '../i18n/index.ts';
import { useClient } from '../core/provider';
import type { ItemDetail } from '../core/types';
import type { AppSpecDto } from '../core/generated/AppSpecDto';
import { Loading, NativeButton, Surface } from '../components/primitives';
import { useTheme } from '../theme';
import { useAppSheetCloseButton } from './app-sheet-context';
import { DECKCRAFT_ENGINE_REVISION } from './canonical-binary';
import { createDeckcraftSession } from './deckcraft-session';
import { shareDeckcraftExport } from './deckcraft-export-files';
import type { MiniappDocumentController } from './document-controller';
import { MiniappFrame } from './frame';
import type { MiniappFrameHandle } from './frame-types';
import { frameAuthority, frameClosures } from './lifetime';
import { readVerifiedMiniappResource } from './resource-load';
import { sandboxHtml, type VerifiedMiniappResource } from './sandbox';

type Props = {
  item: ItemDetail;
  spec: AppSpecDto;
  fill?: boolean;
  onDocumentController?: (controller: MiniappDocumentController | null) => void;
  onDocumentDiscard?: () => void;
};
type RegisterClose = (close: () => void) => () => void;

/** Route focus, not a recycled React row, owns the opaque editor and its leases. */
export function DeckcraftView(props: Props) {
  const [focus, setFocus] = useState<{
    epoch: number;
    lifetime: ReturnType<typeof frameClosures> | null;
  }>({ epoch: 0, lifetime: null });
  useFocusEffect(
    useCallback(() => {
      const lifetime = frameClosures();
      setFocus((prior) => ({ epoch: prior.epoch + 1, lifetime }));
      return () => {
        try {
          lifetime.close();
        } finally {
          setFocus((prior) => (prior.lifetime === lifetime ? { ...prior, lifetime: null } : prior));
        }
      };
    }, []),
  );
  return focus.lifetime ? (
    <FocusedDeckcraft key={focus.epoch} {...props} registerClose={focus.lifetime.register} />
  ) : null;
}

function FocusedDeckcraft(props: Props & { registerClose: RegisterClose }) {
  const { item, spec, registerClose } = props;
  const { store } = useClient();
  const { colors } = useTheme();
  const epoch = useRef(store.sessionEpoch).current;
  const owner = useRef(
    (() => {
      const state = store.getSnapshot();
      return `${state.mode}:${state.snapshot?.account?.identity_id ?? state.snapshot?.me?.id ?? ''}:${state.snapshot?.account?.device_id ?? ''}`;
    })(),
  ).current;
  const [attempt, retry] = useState(0);
  const readKey = `${item.id}:${spec.resource_uri}:${attempt}`;
  const [read, setRead] = useState<{
    key?: string;
    verified?: VerifiedMiniappResource;
    check?: () => void;
    error?: string;
  }>({});
  const checkOwner = useCallback(() => {
    const state = store.getSnapshot();
    const account = state.snapshot?.account;
    if (
      store.sessionEpoch !== epoch ||
      state.phase !== 'ready' ||
      state.erasePending ||
      (state.mode === 'account' && (!account?.unlocked || account.setup_pending)) ||
      `${state.mode}:${account?.identity_id ?? state.snapshot?.me?.id ?? ''}:${account?.device_id ?? ''}` !==
        owner ||
      !state.snapshot?.conversations.some((chat) => chat.id === item.space_id)
    )
      throw Error(t('decks.accountChanged'));
  }, [store, epoch, owner, item.space_id]);
  useEffect(() => {
    let active = true;
    const lifetime = frameAuthority(checkOwner);
    const unregister = registerClose(() => {
      active = false;
      lifetime.close();
    });
    void (async () => {
      lifetime.check();
      if (
        spec.id !== 'deckcraft' ||
        spec.resource_uri !== 'ui://zoen/deckcraft/index.html' ||
        item.app?.app_id !== 'deckcraft'
      )
        throw Error(t('decks.unavailable'));
      const verified = await readVerifiedMiniappResource(
        () => store.driver.request({ op: 'app_resource', uri: spec.resource_uri }),
        'deckcraft',
        spec.resource_uri,
        lifetime.check,
      );
      lifetime.check();
      if (
        !verified.manifest.runtimePackage ||
        verified.manifest.runtimePackage.revision !== DECKCRAFT_ENGINE_REVISION ||
        verified.manifest.capabilities.length ||
        verified.manifest.allowedDomains.length
      )
        throw Error(t('decks.unavailable'));
      if (active) setRead({ key: readKey, verified, check: lifetime.check });
    })().catch((error: unknown) => {
      if (active)
        setRead({
          key: readKey,
          error: error instanceof Error ? error.message : t('decks.unavailable'),
        });
    });
    return () => {
      active = false;
      lifetime.close();
      unregister();
    };
  }, [store, checkOwner, registerClose, readKey, spec.id, spec.resource_uri, item.app?.app_id]);
  if (read.key !== readKey) return <Loading />;
  if (read.error)
    return (
      <Surface>
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {read.error}
        </Text>
        <NativeButton
          secondary
          label={t('miniapps.tryAgain')}
          onPress={() => retry((value) => value + 1)}
        />
      </Surface>
    );
  if (!read.verified || !read.check) return <Loading />;
  return (
    <DeckcraftDocument key={attempt} {...props} verified={read.verified} checkOwner={read.check} />
  );
}

function DeckcraftDocument({
  item,
  fill = false,
  verified,
  checkOwner,
  registerClose,
  onDocumentController,
  onDocumentDiscard,
}: Props & {
  verified: VerifiedMiniappResource;
  checkOwner: () => void;
  registerClose: RegisterClose;
}) {
  const { store } = useClient();
  const { colors, scheme } = useTheme();
  const insets = useSafeAreaInsets();
  const closeButton = useAppSheetCloseButton();
  const safeAreaInsets = useMemo(
    () => (fill ? insets : { top: 0, right: 0, bottom: 0, left: 0 }),
    [fill, insets],
  );
  const [dimensions, setDimensions] = useState({ width: 360, height: 640 });
  const [state, setState] = useState({
    phase: 'saved',
    opened: false,
    blocked: true,
    error: null as string | null,
  });
  const frame = useRef<MiniappFrameHandle>(null);
  const session = useRef<ReturnType<typeof createDeckcraftSession> | null>(null);
  const callbacks = useRef({ onDocumentController });
  const latest = useRef({ item, colors, scheme, safeAreaInsets, closeButton, dimensions });
  useLayoutEffect(() => {
    latest.current = { item, colors, scheme, safeAreaInsets, closeButton, dimensions };
    callbacks.current = { onDocumentController };
  }, [item, colors, scheme, safeAreaInsets, closeButton, dimensions, onDocumentController]);
  const context = useCallback(
    () => ({
      theme: latest.current.scheme,
      displayMode: 'fullscreen',
      availableDisplayModes: ['fullscreen'],
      containerDimensions: latest.current.dimensions,
      safeAreaInsets: latest.current.safeAreaInsets,
      locale,
      platform: process.env.EXPO_OS === 'web' ? 'web' : 'mobile',
      deviceCapabilities: { touch: true, hover: process.env.EXPO_OS === 'web' },
      zoen: { chrome: { closeButton: fill ? latest.current.closeButton : null } },
      styles: {
        variables: {
          '--color-background-primary': latest.current.colors.background,
          '--color-text-primary': latest.current.colors.text,
          '--font-sans': 'system-ui, sans-serif',
        },
      },
    }),
    [fill],
  );
  useLayoutEffect(() => {
    const lifetime = frameAuthority(checkOwner);
    try {
      lifetime.check();
    } catch {
      lifetime.close();
      frame.current?.dispose();
      return;
    }
    const current = createDeckcraftSession({
      itemId: item.id,
      title: () => latest.current.item.title,
      verified,
      driver: store.driver,
      mutate: (command) => store.request(command),
      check: lifetime.check,
      id: () => Crypto.randomUUID(),
      context,
      send: (message) => frame.current?.send(message),
      disposeFrame: () => frame.current?.dispose(),
      close: () => router.back(),
      share: (format, head, call) =>
        shareDeckcraftExport({
          format,
          head,
          call,
          check: lifetime.check,
          title: latest.current.item.title,
        }),
      onState: setState,
    });
    session.current = current;
    callbacks.current.onDocumentController?.(current.controller);
    let live = true;
    const unregisterPreflight = store.registerSessionPreflight(() => {
      if (live) throw Error(t('decks.closeFirst'));
    });
    const close = () => {
      if (!live) return;
      live = false;
      unregisterPreflight();
      lifetime.close();
      current.retire();
      if (session.current === current) session.current = null;
      callbacks.current.onDocumentController?.(null);
    };
    const unregisterClose = registerClose(close);
    const stop = store.subscribe(() => {
      try {
        lifetime.check();
      } catch {
        close();
      }
    });
    const unregister = store.registerSessionCleanup(async () => {
      close();
      await current.drain();
    });
    const background = AppState.addEventListener('change', (next) => {
      if (next !== 'active' && live) void current.controller.flush().catch(() => {});
    });
    return () => {
      stop();
      unregister();
      background.remove();
      close();
      unregisterClose();
    };
  }, [store, verified, item.id, checkOwner, registerClose, context]);
  useEffect(() => {
    session.current?.syncContext();
  }, [scheme, dimensions, safeAreaInsets, closeButton]);
  const html = useMemo(
    () => sandboxHtml(verified.resource.text, [], verified.manifest.runtimePackage),
    [verified],
  );
  const receive = useCallback((message: string) => {
    void session.current?.receive(message);
  }, []);
  const label = !state.opened
    ? t('decks.opening')
    : state.phase === 'saving'
      ? t('decks.saving')
      : state.phase === 'pending'
        ? t('decks.pending')
        : t('decks.saved');
  return (
    <View testID="deckcraft-document" style={fill ? { flex: 1, minHeight: 0 } : { height: 640 }}>
      <Text
        accessibilityLiveRegion="polite"
        testID="deckcraft-save-status"
        style={{
          position: 'absolute',
          width: 1,
          height: 1,
          overflow: 'hidden',
          color: 'transparent',
        }}
      >
        {label}
      </Text>
      <View
        style={{ flex: 1, minHeight: 0 }}
        onLayout={(event) => {
          const { width, height } = event.nativeEvent.layout;
          if (width > 0 && height > 0)
            setDimensions((prior) =>
              prior.width === width && prior.height === height ? prior : { width, height },
            );
        }}
      >
        <MiniappFrame
          ref={frame}
          html={html}
          title={item.title}
          height={dimensions.height}
          fill
          blocked={state.blocked}
          runtimePackage={verified.manifest.runtimePackage}
          onMessage={receive}
          onError={(error) => setState((prior) => ({ ...prior, error }))}
        />
        {!state.opened || state.error ? (
          <View
            testID="deckcraft-interaction-cover"
            style={{
              position: 'absolute',
              inset: 0,
              justifyContent: 'center',
              padding: 24,
              gap: 12,
              backgroundColor: colors.background,
            }}
          >
            {state.error ? (
              <>
                <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
                  {state.error}
                </Text>
                {state.opened ? (
                  <NativeButton
                    secondary
                    label={t('decks.retrySave')}
                    onPress={() => {
                      void session.current?.controller.flush().catch(() => {});
                    }}
                  />
                ) : null}
                {state.opened && onDocumentDiscard ? (
                  <NativeButton secondary label={t('decks.discard')} onPress={onDocumentDiscard} />
                ) : null}
              </>
            ) : (
              <>
                <Loading />
                <Text style={{ color: colors.secondary, textAlign: 'center' }}>{label}</Text>
              </>
            )}
          </View>
        ) : null}
      </View>
    </View>
  );
}
