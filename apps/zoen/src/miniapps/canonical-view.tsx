import { useAppSheetCloseButton } from './app-sheet-context';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import { AppState, Linking, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import * as Crypto from 'expo-crypto';
import { locale, t } from '../i18n/index.ts';
import type { ItemDetail } from '../core/types';
import type { AppSpecDto } from '../core/generated/AppSpecDto';
import { useClient } from '../core/provider';
import { Loading, NativeButton, Surface } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useTheme } from '../theme';
import {
  CanonicalSceneHost,
  type CanonicalSceneDoc,
  type CanonicalScenePhase,
} from './canonical-scene';
import type { MiniappDocumentController } from './document-controller';
import { MiniappFrame } from './frame';
import type { MiniappFrameHandle } from './frame-types';
import { frameAuthority, frameClosures } from './lifetime';
import { MiniappSession } from './protocol';
import { canonicalCopy } from './canonical-copy';
import { canonicalPersistence } from './canonical-persistence';
import { drawingReviewPort } from './drawing-review-port';
import { canonicalRefresh } from './canonical-refresh';
import { safeMiniappLink, sandboxHtml, type VerifiedMiniappResource } from './sandbox';
import {
  canonicalResourcePin,
  isOwnedAppResource,
  readCanonicalResource,
  type CanonicalResourcePin,
} from './canonical-resource';

type Props = {
  item: ItemDetail;
  spec: AppSpecDto;
  fill?: boolean;
  onDocumentController?: (controller: MiniappDocumentController | null) => void;
  onDocumentCopied?: (itemId: string) => void;
  onDocumentDiscard?: () => void;
};
const tools = new Set([
  'app_scene_read',
  'app_scene_read_chunk',
  'app_scene_begin',
  'app_scene_append',
  'app_scene_commit',
  'app_scene_abort',
]);

/** A newly focused document receives a new authority, even if its route stays mounted. */
export function CanonicalDrawingView(props: Props) {
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
    <FocusedDrawing
      key={`${focus.epoch}:${props.item.id}`}
      {...props}
      registerClose={focus.lifetime.register}
    />
  ) : null;
}

function FocusedDrawing({
  item,
  spec,
  fill = false,
  onDocumentController,
  onDocumentCopied,
  onDocumentDiscard,
  registerClose,
}: Props & { registerClose: (end: () => void) => () => void }) {
  const { store } = useClient();
  const { colors } = useTheme();
  const [pin] = useState(() => canonicalResourcePin(item, spec));
  const epoch = useRef(store.sessionEpoch).current;
  const owner = useRef(
    (() => {
      const state = store.getSnapshot();
      return `${state.mode}:${state.snapshot?.account?.identity_id ?? state.snapshot?.me?.id ?? ''}:${state.snapshot?.account?.device_id ?? ''}`;
    })(),
  ).current;
  const [attempt, retry] = useState(0);
  const readKey = `${pin.itemId}:${pin.resourceUri}:${attempt}`;
  const [read, setRead] = useState<{
    key?: string;
    verified?: VerifiedMiniappResource;
    doc?: CanonicalSceneDoc;
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
      throw new Error(t('drawing.accountChanged'));
  }, [store, epoch, owner, item.space_id]);
  useEffect(() => {
    let active = true;
    const lifetime = frameAuthority(checkOwner);
    const unregisterClose = registerClose(() => {
      active = false;
      lifetime.close();
    });
    void (async () => {
      lifetime.check();
      const { verified, doc } = await readCanonicalResource(
        pin,
        (command) => store.driver.request(command),
        lifetime.check,
      );
      if (!active) return;
      lifetime.check();
      setRead({ key: readKey, verified, doc, check: lifetime.check });
    })().catch((error: unknown) => {
      if (active)
        setRead({
          key: readKey,
          error: error instanceof Error ? error.message : t('drawing.unavailable'),
        });
    });
    return () => {
      active = false;
      lifetime.close();
      unregisterClose();
    };
  }, [store, registerClose, checkOwner, pin, readKey]);
  if (read.key !== readKey) return <Loading />;
  if (read.error)
    return (
      <Surface>
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
          {read.error}
        </Text>
        <NativeButton
          secondary
          label={t('miniapps.tryAgain')}
          onPress={() => retry((value) => value + 1)}
        />
      </Surface>
    );
  if (!read.doc || !read.verified || !read.check) return <Loading />;
  return (
    <DrawingDocument
      key={attempt}
      item={item}
      spec={spec}
      fill={fill}
      verified={read.verified}
      doc={read.doc}
      pin={pin}
      checkOwner={read.check}
      registerClose={registerClose}
      onDocumentController={onDocumentController}
      onDocumentCopied={onDocumentCopied}
      onDocumentDiscard={onDocumentDiscard}
    />
  );
}

function DrawingDocument({
  item,
  fill,
  verified,
  doc,
  pin,
  checkOwner,
  registerClose,
  onDocumentController,
  onDocumentCopied,
  onDocumentDiscard,
}: Props & {
  verified: VerifiedMiniappResource;
  doc: CanonicalSceneDoc;
  pin: CanonicalResourcePin;
  checkOwner: () => void;
  registerClose: (end: () => void) => () => void;
}) {
  const { resource, manifest } = verified;
  const { store } = useClient();
  const { colors, scheme } = useTheme();
  const insets = useSafeAreaInsets();
  const closeButton = useAppSheetCloseButton();
  const safeAreaInsets = useMemo(
    () => (fill ? insets : { top: 0, right: 0, bottom: 0, left: 0 }),
    [fill, insets],
  );
  const dialogs = useDialogs();
  const frame = useRef<MiniappFrameHandle>(null);
  const session = useRef<MiniappSession | null>(null);
  const [phase, setPhase] = useState<CanonicalScenePhase>('idle');
  const [pending, setPending] = useState(false);
  const [copyBusy, setCopyBusy] = useState(false);
  const [copyBlocked, setCopyBlocked] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dimensions, setDimensions] = useState({ width: 360, height: 640 });
  const copyAction = useRef<(() => Promise<void>) | null>(null);
  const flushAction = useRef<(() => Promise<void>) | null>(null);
  const callbacks = useRef({ onDocumentController, onDocumentCopied });
  const latest = useRef({ colors, scheme, dimensions, item, safeAreaInsets, closeButton });
  useLayoutEffect(() => {
    latest.current = { colors, scheme, dimensions, item, safeAreaInsets, closeButton };
    callbacks.current = { onDocumentController, onDocumentCopied };
  }, [
    colors,
    scheme,
    dimensions,
    item,
    safeAreaInsets,
    closeButton,
    onDocumentController,
    onDocumentCopied,
  ]);
  const context = useCallback(
    () => ({
      theme: latest.current.scheme,
      displayMode: 'fullscreen',
      availableDisplayModes: ['fullscreen'],
      containerDimensions: latest.current.dimensions,
      locale,
      timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      platform: process.env.EXPO_OS === 'web' ? 'web' : 'mobile',
      deviceCapabilities: { touch: true, hover: process.env.EXPO_OS === 'web' },
      safeAreaInsets: latest.current.safeAreaInsets,
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
    let live = true;
    let retiring = false;
    let refresh: ReturnType<typeof canonicalRefresh> | null = null;
    let review: ReturnType<typeof drawingReviewPort> | null = null;
    const persistence = canonicalPersistence(store, pin, lifetime.check);
    const scene = new CanonicalSceneHost({
      initialDoc: doc,
      id: () => Crypto.randomUUID(),
      check: lifetime.check,
      save: persistence.save,
      onState: (next) => {
        if (!live) return;
        setPhase(next);
        if (next === 'saved') {
          setPending(false);
          setError(null);
        }
        if (next === 'error') setError(t('drawing.saveFailed'));
      },
    });
    const current = new MiniappSession({
      send: (message) => frame.current?.send(message),
      resourceUri: resource.uri,
      tools,
      initialize: () => ({
        protocolVersion: '2026-01-26',
        hostInfo: { name: 'zoen', version: '0.1.0' },
        hostCapabilities: {
          openLinks: {},
          serverTools: {},
          serverResources: {},
          updateModelContext: { structuredContent: {} },
          sandbox: { csp: { connectDomains: [] }, permissions: {} },
          experimental: { zoen: { canonicalScene: { version: 1, itemId: item.id } } },
        },
        hostContext: context(),
      }),
      state: () => ({
        content: [{ type: 'text', text: latest.current.item.title }],
        structuredContent: { checkpointId: item.id },
        isError: false,
      }),
      context,
      resource: async () => {
        lifetime.check();
        return {
          contents: [{ uri: resource.uri, mimeType: resource.mime_type, text: resource.text }],
        };
      },
      call: async (name, args) => {
        lifetime.check();
        const result = await scene.call(name, args);
        lifetime.check();
        return result;
      },
      updateModelContext: async (params) => {
        lifetime.check();
        const value = params.structuredContent as
          | { zoen?: { sceneSave?: { phase?: unknown; sequence?: unknown } } }
          | undefined;
        const state = value?.zoen?.sceneSave;
        refresh?.observe(params.structuredContent);
        review?.observe(params.structuredContent);
        if (state?.phase === 'pending') setPending(true);
        if (state?.phase === 'error') setError(t('drawing.saveFailed'));
        return {};
      },
      openLink: async (input) => {
        lifetime.check();
        const url = safeMiniappLink(input, manifest);
        if (
          !(await dialogs.confirm({
            title: t('miniapps.openLinkOutsideZoen'),
            message: url,
            confirmLabel: t('miniapps.openLink'),
          }))
        )
          throw new Error(t('miniapps.youDidNotOpenTheLink'));
        lifetime.check();
        await Linking.openURL(url);
        lifetime.check();
        return {};
      },
      share: async () => {
        throw new Error(t('miniapps.thisFeatureIsNotAvailableOnThisHost'));
      },
      native: async () => {
        throw new Error(t('miniapps.thisFeatureIsNotAvailableOnThisHost'));
      },
      haptic: () => {},
      resize: () => {},
      close: () => router.back(),
    });
    session.current = current;
    const copy = canonicalCopy({
      itemId: item.id,
      check: lifetime.check,
      id: () => Crypto.randomUUID(),
      prepare: () => current.callAppTool('prepare_scene_copy'),
      staged: (uploadId, bytes, sha) => scene.stagedScene(uploadId, bytes, sha),
      save: persistence.copy,
      blocked: (value) => {
        if (live) setCopyBlocked(value);
      },
    });
    copyAction.current = async () => {
      if (copy.busy || retiring) return;
      setCopyBusy(true);
      try {
        const accepted = await (refresh ? refresh.barrier(() => copy.run()) : copy.run());
        lifetime.check();
        callbacks.current.onDocumentCopied?.(accepted.item_id);
      } catch {
        if (live) setError(t('drawing.copyFailed'));
      } finally {
        if (live) setCopyBusy(false);
      }
    };
    let controller: MiniappDocumentController = {
      ...(isOwnedAppResource(pin.resourceUri)
        ? { agentDocument: { kind: 'owned-scene' as const, resourceUri: pin.resourceUri } }
        : {}),
      flush: async (options) => {
        const work = async () => {
          lifetime.check();
          if (copy.busy || copy.unresolved) throw new Error(t('drawing.copyFailed'));
          if (!current.ready) return;
          if (options?.retire) retiring = true;
          try {
            await current.flushResource();
            lifetime.check();
            if (!retiring) {
              const resumed = await current.callAppTool('resume_scene');
              lifetime.check();
              if (resumed.isError) throw new Error(t('drawing.saveFailed'));
            }
          } catch (error) {
            if (options?.retire) retiring = false;
            if (live) setError(t('drawing.saveFailed'));
            throw error;
          }
          lifetime.check();
          if (live) {
            setError(null);
            setPending(false);
          }
        };
        if (refresh) await refresh.refresh();
        return refresh ? refresh.barrier(work) : work();
      },
    };
    if (isOwnedAppResource(pin.resourceUri))
      refresh = canonicalRefresh({
        itemId: item.id,
        scene,
        check: lifetime.check,
        read: persistence.read,
        call: (name, args) => current.callAppTool(name, args),
        conflict: () => {
          if (live) setError(t('drawing.saveFailed'));
        },
      });
    if (refresh)
      controller = {
        ...controller,
        observedHead: refresh.observedHead,
        agentReadiness: refresh.readiness,
      };
    if (isOwnedAppResource(pin.resourceUri)) {
      review = drawingReviewPort({
        store,
        pin,
        spaceId: item.space_id,
        key: Crypto.randomUUID(),
        id: () => Crypto.randomUUID(),
        check: lifetime.check,
        call: (name, args) => current.callAppTool(name, args),
      });
      controller = { ...controller, drawingReview: review };
    }
    flushAction.current = controller.flush;
    callbacks.current.onDocumentController?.(controller);
    const unregisterPreflight = store.registerSessionPreflight(() => {
      if (live) throw new Error(t('drawing.closeFirst'));
    });
    const close = () => {
      if (!live) return;
      live = false;
      unregisterPreflight();
      lifetime.close();
      refresh?.dispose();
      review?.dispose();
      copy.dispose();
      copyAction.current = null;
      flushAction.current = null;
      scene.dispose();
      current.dispose();
      frame.current?.dispose();
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
    const stopChanges = store.subscribeCanonicalChanges((spaces) => {
      if (!refresh || (spaces?.length && !spaces.includes(item.space_id))) return;
      void refresh.refresh().catch(() => {
        if (live) setError(t('drawing.saveFailed'));
      });
    });
    const unregister = store.registerSessionCleanup(async () => {
      close();
    });
    const background = AppState.addEventListener('change', (state) => {
      if (state !== 'active' && live)
        void controller.flush().catch(() => {
          if (live) setError(t('drawing.saveFailed'));
        });
    });
    return () => {
      stop();
      stopChanges();
      unregister();
      background.remove();
      close();
      unregisterClose();
    };
  }, [
    store,
    checkOwner,
    context,
    doc,
    item.id,
    item.space_id,
    resource,
    manifest,
    dialogs,
    registerClose,
    pin,
  ]);
  useEffect(() => {
    session.current?.syncContext();
  }, [scheme, dimensions, safeAreaInsets, closeButton]);
  const html = useMemo(() => sandboxHtml(resource.text, []), [resource.text]);
  const receive = useCallback((message: string) => {
    void session.current?.receive(message);
  }, []);
  const label =
    phase === 'saving' ? t('drawing.saving') : pending ? t('drawing.pending') : t('drawing.saved');
  return (
    <View style={fill ? { flex: 1, minHeight: 0 } : { height: 640 }}>
      {error ? (
        <View style={{ padding: 12, gap: 8 }}>
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
            {error}
          </Text>
          <NativeButton
            secondary
            label={t('drawing.retrySave')}
            disabled={copyBlocked}
            onPress={() => {
              void flushAction
                .current?.()
                .then(() => {
                  setError(null);
                  setPending(false);
                })
                .catch(() => setError(t('drawing.saveFailed')));
            }}
          />
          {onDocumentCopied ? (
            <NativeButton
              secondary
              testID="drawing-save-copy"
              label={copyBusy ? t('drawing.copying') : t('drawing.saveCopy')}
              disabled={copyBusy}
              onPress={() => {
                void copyAction.current?.();
              }}
            />
          ) : null}
          {onDocumentDiscard ? (
            <NativeButton
              secondary
              label={t('drawing.discard')}
              disabled={copyBusy}
              onPress={onDocumentDiscard}
            />
          ) : null}
        </View>
      ) : null}
      {/* Save announcements do not reserve space above the canvas. */}
      <Text
        accessibilityLiveRegion="polite"
        testID="drawing-save-status"
        style={{
          position: 'absolute',
          width: 1,
          height: 1,
          overflow: 'hidden',
          color: 'transparent',
        }}
      >
        {copyBusy ? t('drawing.copying') : label}
      </Text>
      <View
        style={{ flex: 1, minHeight: 0 }}
        onLayout={(event) => {
          const { width, height } = event.nativeEvent.layout;
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
          blocked={copyBlocked}
          onMessage={receive}
          onError={setError}
        />
      </View>
    </View>
  );
}
