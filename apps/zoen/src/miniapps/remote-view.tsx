import { locale, t } from '../i18n/index.ts';
import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { Stack, useFocusEffect, usePreventRemove } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Linking, Text, View } from 'react-native';
import { fetch as expoFetch } from 'expo/fetch';
import type { CallToolResult } from '@modelcontextprotocol/client';
import { useClient } from '../core/provider';
import { EmptyState, Loading, NativeButton, Surface } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useTheme } from '../theme';
import { MiniappFrame } from './frame';
import type { MiniappFrameHandle } from './frame-types';
import { frameAuthority } from './lifetime';
import { MiniappSession } from './protocol';
import { remoteAppHtml } from './remote-sandbox';
import { excalidrawActionDecision } from './remote-grants';
import { excalidrawPresentation } from './excalidraw-presentation';
import {
  createRemoteMcpSession,
  type RemoteAppResource,
  type RemoteMcpDescriptor,
} from './remote-session';

/** An external app never receives a Zoen account, chat, member list or decryption key. */
export function RemoteAppScreen({ id }: { id: string }) {
  const [focus, setFocus] = useState({ active: false, epoch: 0 });
  const end = useRef<(() => void) | null>(null);
  const registerClose = useCallback((close: (() => void) | null) => {
    end.current = close;
  }, []);
  useFocusEffect(
    useCallback(() => {
      setFocus((previous) => ({ active: true, epoch: previous.epoch + 1 }));
      return () => {
        end.current?.();
        setFocus((previous) => ({ ...previous, active: false }));
      };
    }, []),
  );
  if (id !== 'excalidraw')
    return <EmptyState title={t('connectedApps.unsupportedApp')} detail="" />;
  return focus.active ? <DrawingSession key={focus.epoch} registerClose={registerClose} /> : null;
}

type Drawing = {
  descriptor: RemoteMcpDescriptor;
  resource: RemoteAppResource;
  result: CallToolResult;
  arguments: Record<string, unknown>;
  html: string;
  checkpoint: string;
};

function DrawingSession({
  registerClose,
}: {
  registerClose: (close: (() => void) | null) => void;
}) {
  const { store, snapshot, mode } = useClient();
  const { colors, scheme } = useTheme();
  const header = useHeaderHeight();
  const insets = useSafeAreaInsets();
  const dialogs = useDialogs();
  const frame = useRef<MiniappFrameHandle>(null);
  const protocol = useRef<MiniappSession | null>(null);
  const remote = useRef<ReturnType<typeof createRemoteMcpSession> | null>(null);
  const authority = useRef<ReturnType<typeof frameAuthority> | null>(null);
  const [drawing, setDrawing] = useState<Drawing | null>(null);
  const [busy, setBusy] = useState(false);
  const starting = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const [ended, setEnded] = useState(false);
  const [saveStatus, setSaveStatus] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle');
  const [leaving, setLeaving] = useState(false);
  const confirming = useRef(false);
  const [dimensions, setDimensions] = useState({ width: 360, height: 640 });
  const displayMode = useRef<'inline' | 'fullscreen'>('fullscreen');
  const modelContext = useRef<Record<string, unknown> | null>(null);
  const latest = useRef({ colors, scheme, dimensions });
  useLayoutEffect(() => {
    latest.current = { colors, scheme, dimensions };
  }, [colors, scheme, dimensions]);
  const owner = useRef(
    `${mode}:${snapshot?.account?.identity_id ?? ''}:${snapshot?.account?.device_id ?? ''}`,
  ).current;
  const checkOwner = useCallback(() => {
    const current = store.getSnapshot();
    const account = current.snapshot?.account;
    if (
      !account?.unlocked ||
      account.setup_pending ||
      `${current.mode}:${account.identity_id}:${account.device_id}` !== owner
    )
      throw new Error(t('connectedApps.accountChanged'));
  }, [store, owner]);
  const context = useCallback(
    () => ({
      theme: latest.current.scheme,
      displayMode: displayMode.current,
      availableDisplayModes: ['inline', 'fullscreen'],
      containerDimensions: latest.current.dimensions,
      locale,
      timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      platform: process.env.EXPO_OS === 'web' ? 'web' : 'mobile',
      deviceCapabilities: { touch: true, hover: process.env.EXPO_OS === 'web' },
      safeAreaInsets: { top: 0, bottom: 0, left: 0, right: 0 },
      styles: {
        variables: {
          '--color-background-primary': latest.current.colors.background,
          '--color-text-primary': latest.current.colors.text,
          '--font-sans': 'system-ui, sans-serif',
        },
      },
    }),
    [],
  );
  usePreventRemove(!!drawing && !ended && !leaving, ({ repeat }) => {
    if (confirming.current) return;
    confirming.current = true;
    void dialogs
      .confirm({
        title: t('connectedApps.endSession'),
        message: t('connectedApps.endSessionDetails'),
        confirmLabel: t('connectedApps.end'),
        cancelLabel: t('connectedApps.keepDrawing'),
      })
      .then((accepted) => {
        if (accepted) {
          setLeaving(true);
          repeat();
        }
      })
      .finally(() => {
        confirming.current = false;
      });
  });
  useLayoutEffect(() => {
    const lifetime = frameAuthority(checkOwner);
    authority.current = lifetime;
    const close = () => {
      lifetime.close();
      protocol.current?.dispose();
      protocol.current = null;
      frame.current?.dispose();
      const connection = remote.current;
      remote.current = null;
      void connection?.close().catch(() => {});
      modelContext.current = null;
    };
    registerClose(close);
    const unsubscribe = store.subscribe(() => {
      try {
        checkOwner();
      } catch {
        close();
        setEnded(true);
      }
    });
    return () => {
      unsubscribe();
      close();
      if (authority.current === lifetime) authority.current = null;
      registerClose(null);
    };
  }, [checkOwner, registerClose, store]);

  const start = () => {
    if (starting.current) return;
    const check = authority.current?.check;
    if (!check) return;
    starting.current = true;
    setBusy(true);
    setError(null);
    let connection: ReturnType<typeof createRemoteMcpSession> | null = null;
    void (async () => {
      check();
      connection = createRemoteMcpSession('excalidraw', check, {
        fetch: expoFetch as typeof fetch,
      });
      remote.current = connection;
      const descriptor = await connection.connect();
      check();
      const upstreamResource = await connection.readApp();
      check();
      const resource = excalidrawPresentation(descriptor, upstreamResource);
      // Excalidraw's official editor needs an initial element. This is ordinary
      // tool input; the provider's resource and its editor are never rewritten.
      const args = {
        elements: JSON.stringify([
          {
            id: 'idea',
            type: 'rectangle',
            x: 0,
            y: 0,
            width: 240,
            height: 120,
            backgroundColor: '#e3f3e8',
            strokeColor: '#1e1e1e',
            label: { text: t('connectedApps.idea') },
          },
        ]),
      };
      if (descriptor.mainToolName !== 'create_view')
        throw new Error(t('connectedApps.unavailable'));
      const result = await connection.call(descriptor.mainToolName, args, { origin: 'host' });
      check();
      if (result.isError) throw new Error(t('connectedApps.unavailable'));
      const structured = result.structuredContent;
      const checkpoint =
        structured && typeof structured === 'object' && 'checkpointId' in structured
          ? structured.checkpointId
          : undefined;
      if (typeof checkpoint !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(checkpoint))
        throw new Error(t('connectedApps.unavailable'));
      const html = remoteAppHtml(resource.html, resource.csp);
      setDrawing({ descriptor, resource, result, arguments: args, html, checkpoint });
    })()
      .catch((reason: unknown) => {
        try {
          check();
        } catch {
          return;
        }
        if (remote.current === connection) remote.current = null;
        void connection?.close().catch(() => {});
        setError(reason instanceof Error ? reason.message : t('connectedApps.unavailable'));
      })
      .finally(() => {
        starting.current = false;
        try {
          check();
          setBusy(false);
        } catch {
          /* The closed owner publishes no late state. */
        }
      });
  };

  useLayoutEffect(() => {
    if (!drawing || ended) return;
    const check = authority.current?.check;
    const connection = remote.current;
    if (!check || !connection) return;
    // Checkpoint writes preserve edit order even on a slow connection.
    let saveTail: Promise<unknown> = Promise.resolve();
    let latestSave = 0;
    const current = new MiniappSession({
      send: (message) => frame.current?.send(message),
      initialize: () => ({
        protocolVersion: '2026-01-26',
        hostInfo: { name: 'Zoen', version: '0.1.0' },
        hostCapabilities: {
          serverTools: {},
          serverResources: {},
          openLinks: {},
          updateModelContext: { text: {} },
          sandbox: { csp: drawing.resource.csp, permissions: {} },
        },
        hostContext: context(),
      }),
      arguments: () => drawing.arguments,
      state: () => drawing.result,
      context,
      tools: new Set(drawing.descriptor.appTools.map((tool) => tool.name)),
      resourceUri: drawing.resource.resourceUri,
      resource: async () => {
        check();
        return {
          contents: [
            {
              uri: drawing.resource.resourceUri,
              mimeType: drawing.resource.mimeType,
              text: drawing.resource.html,
              _meta: {
                ui: {
                  csp: drawing.resource.csp,
                  permissions: drawing.resource.permissions,
                },
              },
            },
          ],
        };
      },
      call: async (name, args) => {
        check();
        const tool = drawing.descriptor.appTools.find((candidate) => candidate.name === name);
        if (!tool) throw new Error(t('connectedApps.unavailableAction'));
        const decision = excalidrawActionDecision(tool, args, drawing.checkpoint);
        const publication = decision === 'publish';
        if (
          decision !== 'session' &&
          !(await dialogs.confirm({
            title: publication
              ? t('connectedApps.publishDrawing')
              : t('connectedApps.confirmAction'),
            message: publication
              ? t('connectedApps.publishDrawingDetails')
              : (tool.description?.slice(0, 1000) ?? t('connectedApps.externalActionDetails')),
            confirmLabel: t('connectedApps.allow'),
          }))
        )
          throw new Error(t('miniapps.youCanceledThisAction'));
        check();
        if (name === 'save_checkpoint') {
          const revision = ++latestSave;
          setSaveStatus('saving');
          const save = saveTail.then(async () => {
            check();
            const result = await connection.call(name, args);
            check();
            if (result.isError) throw new Error(t('connectedApps.saveFailed'));
            return result;
          });
          saveTail = save.catch(() => {});
          try {
            const result = await save;
            check();
            if (revision === latestSave) {
              setSaveStatus('saved');
              setError(null);
            }
            return result;
          } catch (reason) {
            check();
            if (revision === latestSave) {
              setSaveStatus('error');
              setError(t('connectedApps.saveFailed'));
            }
            throw reason;
          }
        }
        if (name === 'read_checkpoint') {
          await saveTail;
          check();
        }
        return connection.call(name, args);
      },
      updateModelContext: async (params) => {
        check();
        modelContext.current = params;
        return {};
      },
      openLink: async (input) => {
        check();
        if (typeof input !== 'string' || input.length > 4096)
          throw new Error(t('miniapps.invalidLink'));
        const url = new URL(input);
        if (url.protocol !== 'https:' || url.username || url.password)
          throw new Error(t('miniapps.invalidLink'));
        if (
          !(await dialogs.confirm({
            title: t('connectedApps.openLink'),
            message: url.href,
            confirmLabel: t('connectedApps.open'),
          }))
        )
          throw new Error(t('miniapps.youCanceledThisAction'));
        check();
        await Linking.openURL(url.href);
        return {};
      },
      share: async () => {
        throw new Error(t('connectedApps.unavailableAction'));
      },
      native: async () => {
        throw new Error(t('connectedApps.unavailableAction'));
      },
      haptic: () => {},
      resize: () => {},
      close: () => {},
      requestDisplayMode: async (mode) => {
        check();
        displayMode.current = mode === 'inline' ? 'inline' : 'fullscreen';
        current.syncContext();
        return { mode: displayMode.current };
      },
    });
    protocol.current = current;
    return () => {
      current.dispose();
      if (protocol.current === current) protocol.current = null;
    };
  }, [context, dialogs, drawing, ended]);
  useLayoutEffect(() => {
    protocol.current?.syncContext();
  }, [scheme, dimensions]);
  const receive = useCallback((message: string) => {
    void protocol.current?.receive(message);
  }, []);

  return (
    <View
      testID="connected-app-screen"
      style={{
        flex: 1,
        paddingTop: header,
        paddingBottom: insets.bottom,
        paddingLeft: insets.left,
        paddingRight: insets.right,
      }}
    >
      <Stack.Screen options={{ title: 'Excalidraw' }} />
      {ended ? (
        <EmptyState title={t('connectedApps.sessionEnded')} detail="" />
      ) : drawing ? (
        <>
          <Text
            testID="connected-app-save-status"
            accessibilityLiveRegion="polite"
            style={{
              color: colors.secondary,
              fontSize: 12,
              paddingHorizontal: 16,
              paddingVertical: 8,
            }}
          >
            {saveStatus === 'saved'
              ? t('connectedApps.saved')
              : saveStatus === 'saving'
                ? t('connectedApps.saving')
                : saveStatus === 'error'
                  ? t('connectedApps.saveFailed')
                  : t('connectedApps.sessionDrawing')}
          </Text>
          {error ? (
            <Text
              accessibilityRole="alert"
              selectable
              style={{ color: colors.danger, padding: 16 }}
            >
              {error}
            </Text>
          ) : null}
          <View
            style={{ flex: 1, minHeight: 0 }}
            onLayout={(event) => {
              const { width, height } = event.nativeEvent.layout;
              if (width > 0 && height > 0)
                setDimensions((previous) =>
                  previous.width === width && previous.height === height
                    ? previous
                    : { width, height },
                );
            }}
          >
            <MiniappFrame
              ref={frame}
              html={drawing.html}
              title="Excalidraw"
              height={dimensions.height}
              fill
              blocked={false}
              onMessage={receive}
              onError={setError}
            />
          </View>
        </>
      ) : (
        <View style={{ padding: 24, gap: 20 }}>
          <Surface>
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 28, fontWeight: '600' }}
            >
              Excalidraw
            </Text>
            <Text style={{ color: colors.secondary, fontSize: 17, lineHeight: 25 }}>
              {t('connectedApps.excalidrawDescription')}
            </Text>
            <Text style={{ color: colors.secondary, fontSize: 14, lineHeight: 21 }}>
              {t('connectedApps.externalDrawingDetails')}
            </Text>
            <NativeButton
              testID="start-excalidraw-drawing"
              disabled={busy || !snapshot?.account?.unlocked}
              label={error ? t('connectedApps.retry') : t('connectedApps.startDrawing')}
              onPress={start}
            />
          </Surface>
          {busy ? <Loading label={t('connectedApps.connecting')} /> : null}
          {error ? (
            <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
              {error}
            </Text>
          ) : null}
        </View>
      )}
    </View>
  );
}
