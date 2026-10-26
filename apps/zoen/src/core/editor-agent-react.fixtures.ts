import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import * as React from 'react';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { editorAgentTargets } from './editor-agent-targets.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import { EditorAgentSession } from '../miniapps/editor-agent-session.ts';
import * as port from '../miniapps/editor-agent-port.ts';
import * as formats from '../miniapps/binary-formats.ts';
import * as heads from '../miniapps/deck-version-selection.ts';
import * as memberFlow from '../communities/member-actions.ts';
import * as additionFlow from '../communities/member-addition.ts';
import * as memberPort from '../communities/member-context-port.ts';
import * as ownership from './account-scope.ts';
import * as decisions from './tool-request-decision.ts';
import * as sourceScope from '../sites/source.ts';
import { drawingReviewPort } from '../miniapps/drawing-review-port.ts';
import { drawingReview, drawingReply } from './drawing-review.fixtures.ts';
import * as enrollmentFlow from '../agent-enrollment/session.ts';
import * as enrollmentPort from '../agent-enrollment/core-port.ts';
import * as trust from '../agents/trust-confirmation.ts';
import * as admission from '../miniapps/editor-agent-admission.ts';
import { editorAgentMembers, type AgentMemberMode } from './editor-agent-members.fixtures.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import { t } from '../i18n/index.ts';
import type { ItemDetail, TimelineEntry } from './types';
import type { MiniappDocumentController } from '../miniapps/document-controller';
import type { AgentRequestDto } from './generated/AgentRequestDto';

const require = createRequire(import.meta.url);
type Control = Record<string, any>;
/** Real compiled screen/sheet + React/Store + real session/port; native controls
 * and Core are explicit ports, not a claim of iPhone presentation or AI execution. */
export async function editorAgentReactFixture(
  options: { ownedScene?: boolean; ready?: boolean; visualReview?: boolean } = {},
) {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const space = snapshot.conversations[0]!.id,
    itemId = 'word-item';
  const memberModel = editorAgentMembers(snapshot, space);
  const targetModel = editorAgentTargets(snapshot, space, memberModel, () =>
    driver.changed?.({ kind: 'change', spaceIds: [space] }),
  );
  const calls: any[] = [],
    routes: any[] = [],
    controls = new Map<string, Control>();
  const item = {
    id: itemId,
    space_id: space,
    space_title: 'Conversation',
    title: 'My document',
    version: 1,
    app: { app_id: options.ownedScene ? 'excalidraw' : 'wordcraft' },
  } as ItemDetail;
  const spec = {
    id: options.ownedScene ? 'excalidraw' : 'wordcraft',
    has_view: true,
    tools: [],
    resource_uri: 'ui://zoen/wordcraft/index.html',
  };
  let flushes = 0,
    mounts = 0,
    unmounts = 0,
    ids = 0,
    presented = false,
    sheetMounts = 0;
  let editConflict: string | null = null;
  let editConflictKind: 'MissingOwnedAgent' | 'WaitingForMls' | 'Changed' = 'Changed';
  let remoteEntries: TimelineEntry[] = [],
    remoteRequests: AgentRequestDto[] = [];
  let sceneReady = options.ready !== false;
  const sceneListeners = new Set<() => void>();
  const resourceUri = `ui://zoen/package/file/1/${'ef'.repeat(32)}`;
  let sceneVersion = 1,
    sceneSha = 'ab'.repeat(32);
  let document: MiniappDocumentController = {
    ...(options.ownedScene
      ? {
          agentDocument: { kind: 'owned-scene', resourceUri } as const,
          agentReadiness: {
            getSnapshot: () => sceneReady,
            subscribe: (listener: () => void) => {
              sceneListeners.add(listener);
              return () => {
                sceneListeners.delete(listener);
              };
            },
          },
        }
      : {}),
    observedHead: async () => ({ itemId, version: sceneVersion, sha256: sceneSha }),
    flush: async (options?: unknown) => {
      assert.equal(options, undefined);
      flushes++;
    },
  };
  let previewAllowed = true;
  let renderGate: Promise<void> | null = null;
  const review = drawingReview(space, itemId);
  if (options.visualReview) sceneSha = review.before_sha256;
  driver.opening = async () => snapshot;
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    if (targetModel.handles(command.op))
      return targetModel.handle(
        command,
        sceneVersion,
        sceneSha,
        options.ownedScene ? resourceUri : undefined,
      );
    if (memberModel.handles(command.op)) return memberModel.handle(command);
    switch (command.op) {
      case 'snapshot':
        return snapshot;
      case 'requests':
        return remoteRequests;
      case 'timeline':
        return { entries: remoteEntries, before: null, has_more: false };
      case 'thread':
        return remoteEntries;
      case 'scene_tool_review':
        assert.equal(command.input.expected_resource_uri, resourceUri);
        if (!previewAllowed) throw Error('The reviewed drawing changed.');
        return {
          ...review,
          request_id: command.input.request_id,
          content_hash: command.input.expected_hash,
        };
      case 'decide_tool_request':
        if (!previewAllowed) throw Error('The reviewed drawing changed.');
        return { message: 'Approved', also_resolved: 0 };
      case 'app_document_read_pinned':
        assert.equal(command.expected_resource_uri, resourceUri);
        return {
          item_id: itemId,
          version: sceneVersion,
          current_version: sceneVersion,
          sha256: sceneSha,
          scene_json: '{}',
        };
      case 'app_binary_read':
        return {
          item_id: itemId,
          kind: 'wordcraft',
          version: 1,
          current_version: 1,
          sha256: 'ab'.repeat(32),
          byte_length: 0,
          data_base64: '',
          engine_revision: formats.WORDCRAFT_ENGINE_REVISION,
        };
      case 'request_app_edit':
        targetModel.checkRequest(
          command,
          sceneVersion,
          sceneSha,
          options.ownedScene ? resourceUri : undefined,
        );
        if (editConflict)
          return { entry: null, conflict: editConflict, conflict_kind: editConflictKind };
        return {
          entry: {
            id:
              calls.filter(({ op }) => op === 'request_app_edit').length === 1
                ? 'canonical-request'
                : `canonical-${command.mutation_id}`,
            delivery: 'Sending',
            kind: { Message: { text: command.text, card: null } },
          },
          conflict: null,
        };
      default:
        throw Error(`Unexpected operation ${command.op}`);
    }
  };
  const store = new ClientStore(driver);
  await store.open('account');
  calls.length = 0;
  const previewPort = options.visualReview
    ? drawingReviewPort({
        store,
        pin: { itemId, resourceUri, appId: 'excalidraw' },
        spaceId: space,
        key: 'focused-frame',
        id: () => `preview-${++ids}`,
        check: () => {},
        call: async (_name, args) => {
          if (renderGate) await renderGate;
          return drawingReply(args);
        },
      })
    : null;
  if (previewPort) {
    previewPort.observe({ zoen: { sceneSave: { itemId, ready: true, reviewPreview: 1 } } });
    document = { ...document, drawingReview: previewPort };
  }
  const children = ({ children }: { children: React.ReactNode }) => children;
  const visibility = React.createContext(true);
  const appListeners = new Set<(state: string) => void>();
  let appState = 'active';
  function VisibleView(props: Control) {
    const parent = React.useContext(visibility);
    const hidden = [props.style].flat().some((style) => style?.display === 'none');
    return React.createElement(visibility.Provider, { value: parent && !hidden }, props.children);
  }
  function Capture(props: Control) {
    const visible = React.useContext(visibility);
    const key = visible
      ? (props.testID ?? (props.label ? `label:${props.label}` : undefined))
      : undefined;
    React.useLayoutEffect(() => {
      if (key) controls.set(key, props);
      return () => {
        if (key && controls.get(key) === props) controls.delete(key);
      };
    }, [key, props]);
    return null;
  }
  function Sheet(props: Control) {
    React.useEffect(() => {
      sheetMounts++;
      return () => {
        sheetMounts--;
      };
    }, []);
    React.useLayoutEffect(() => {
      presented = props.isPresented;
      controls.set('sheet', props);
    }, [props]);
    return props.isPresented ? props.children : null;
  }
  function Editor({ onDocumentController }: Control) {
    React.useEffect(() => {
      mounts++;
      onDocumentController(document);
      return () => {
        unmounts++;
        onDocumentController(null);
      };
    }, [onDocumentController]);
    return null;
  }
  const colors = {
    text: '#fff',
    secondary: '#888',
    danger: '#f00',
    background: '#111',
    surface: '#222',
    accent: '#0af',
  };
  const common: Record<string, any> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': {
      View: VisibleView,
      Pressable: (props: Control) =>
        React.createElement(
          React.Fragment,
          null,
          React.createElement(Capture, props),
          props.children,
        ),
      FlatList: (props: Control) =>
        React.createElement(
          React.Fragment,
          null,
          props.data.map((item: unknown, key: number) =>
            React.createElement(React.Fragment, { key }, props.renderItem({ item })),
          ),
        ),
      Keyboard: { dismiss() {} },
      Platform: { OS: 'ios' },
      Text: (props: Control) =>
        React.createElement(Capture, {
          ...props,
          testID: typeof props.children === 'string' ? `text:${props.children}` : undefined,
        }),
      ScrollView: children,
      useWindowDimensions: () => ({ width: 402, height: 874 }),
      AppState: {
        get currentState() {
          return appState;
        },
        addEventListener: (_: string, callback: (state: string) => void) => {
          appListeners.add(callback);
          return {
            remove() {
              appListeners.delete(callback);
            },
          };
        },
      },
    },
    '@expo/ui': {
      Host: children,
      RNHostView: children,
      BottomSheet: Sheet,
      Picker: Object.assign(
        (props: Control) =>
          React.createElement(
            React.Fragment,
            null,
            React.createElement(Capture, props),
            props.children,
          ),
        { Item: Capture },
      ),
    },
    'expo-router': {
      useFocusEffect: (work: () => void | (() => void)) => React.useEffect(work, [work]),
    },
    'expo-camera': {
      CameraView: {
        isModernBarcodeScannerAvailable: true,
        onModernBarcodeScanned: () => ({ remove() {} }),
        dismissScanner: async () => {},
        launchScanner: async () => {},
      },
      useCameraPermissions: () => [{ granted: true }, async () => ({ granted: true })],
    },
    'expo-crypto': { randomUUID: () => `request-${++ids}` },
    'react-native-safe-area-context': {
      useSafeAreaInsets: () => ({ top: 40, bottom: 30, left: 0, right: 0 }),
    },
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../theme': {
      useTheme: () => ({ colors, scheme: 'dark', presentation: telegramPresentation('night') }),
    },
    '../i18n': { t },
    '../i18n/index.ts': { t },
    '../components/primitives': {
      IconButton: Capture,
      NativeButton: Capture,
      Field: Capture,
      Icon: () => null,
      Avatar: () => null,
      Loading: () => null,
      Surface: children,
    },
    '../components/composer-text-field': { ComposerTextField: Capture },
    '../components/message-markdown': {
      MessageMarkdown: (props: Control) =>
        React.createElement(Capture, { ...props, testID: `markdown:${props.text}` }),
    },
    '../components/dialogs': {
      useDialogs: () => ({
        confirm: async () => {
          throw Error('No stacked confirmation dialog is permitted');
        },
      }),
    },
    '../components/settings-rows': {
      SettingsRows: (props: Control) =>
        React.createElement(
          React.Fragment,
          null,
          props.rows.map((row: Control) =>
            React.createElement(Capture, { ...row, key: row.id, testID: row.id }),
          ),
        ),
    },
    '../components/settings-screen': { SettingsScreen: children },
    '../profile/profile-avatar': { ProfileAvatar: () => null },
    '../profile/profile-header': { ProfileHeader: () => null },
    '../profile/profile-section': { ProfileSection: children },
    '../core/account-scope': ownership,
    '../core/account-scope.ts': ownership,
    '../core/tool-request-decision': decisions,
    '../core/presentation': { money: () => '' },
    '../core/image-tool-review': { imageToolReviewValid: () => false },
    '../sites/source': sourceScope,
    './site-tool-review': { SiteToolReview: () => null },
    './tool-review-recovery': { ToolReviewRecovery: () => null },
    'expo-image': { Image: Capture },
    './member-actions': memberFlow,
    './member-addition': additionFlow,
    './member-context-port': memberPort,
    './session': enrollmentFlow,
    './core-port': enrollmentPort,
    './editor-agent-admission': admission,
    '../agents/request-card': {
      RequestCard: (props: Control) =>
        React.createElement(Capture, { ...props, testID: `review:${props.request.id}` }),
    },
    './app-sheet-context': {
      useAppSheetCloseButton: () => ({ top: 48, right: 12, width: 44, height: 44 }),
    },
    './document-controller': {},
    './package-release-sheet': { AppPackageReleaseSheet: () => null },
    './editor-agent-port': port,
    './editor-agent-session': { EditorAgentSession },
    './binary-formats': formats,
    './deck-version-selection': heads,
  };
  const load = (path: string, overrides: Record<string, any>) =>
    loadEditorAgent(path, { ...common, ...overrides });
  common['../communities/use-member-actions'] = load('../communities/use-member-actions.ts', {});
  common['../communities/member-addition-sheet'] = load(
    '../communities/member-addition-sheet.tsx',
    {},
  );
  common['../agent-enrollment/connect-agent-view'] = load(
    '../agent-enrollment/connect-agent-view.tsx',
    {},
  );
  common['../agent-enrollment/use-agent-enrollment'] = load(
    '../agent-enrollment/use-agent-enrollment.ts',
    {},
  );
  if (options.visualReview) {
    common['./drawing-tool-review'] = load('../agents/drawing-tool-review.tsx', {});
    common['../agents/request-card'] = load('../agents/request-card.tsx', {
      'expo-router': {
        ...common['expo-router'],
        router: { push: (...args: any[]) => routes.push(args) },
      },
    });
  }
  common['./editor-agent-choice'] = load('../miniapps/editor-agent-choice.tsx', {});
  common['./editor-agent-permissions'] = load('../miniapps/editor-agent-permissions.tsx', {
    '../agents/trust-confirmation': trust,
    '../components/dialogs': { useDialogs: () => ({ confirm: targetModel.confirm }) },
  });
  common['./editor-agent-content'] = load('../miniapps/editor-agent-content.tsx', {});
  common['./editor-agent-enrollment'] = load('../miniapps/editor-agent-enrollment.tsx', {});
  const sheet = load('../miniapps/editor-agent-sheet.tsx', {});
  const screen = load('../miniapps/detail-screen.tsx', {
    'expo-router': {
      Stack: { Screen: () => null },
      router: {
        push: (...args: any[]) => routes.push(args),
        replace: (...args: any[]) => routes.push(args),
        back: () => routes.push('back'),
      },
      useLocalSearchParams: () => ({ id: itemId }),
      usePreventRemove: () => () => {},
    },
    '../core/query': {
      useCoreQuery: (command: Control) => ({
        data: command.op === 'item' ? item : [spec],
        retry() {},
        error: null,
      }),
    },
    '../components/dialogs': { useDialogs: () => ({ confirm: async () => true }) },
    '../home/provider': { useHome: () => ({}) },
    '../home/menus': { HomeCardMenu: () => null },
    '../native-surfaces/controls': { MiniappSurfaceControls: () => null },
    './conversation-provider': { useConversationCards: () => ({}) },
    './state-view': { MiniappState: () => null },
    './view-host': { MiniappView: Editor },
    './actions': { callMiniapp: async () => null },
    './editor-agent-sheet': sheet,
  });
  return {
    store,
    review,
    previewAllowed(value: boolean) {
      previewAllowed = value;
    },
    holdPreview(gate: Promise<void> | null) {
      renderGate = gate;
    },
    sceneReady(ready: boolean, version = sceneVersion, sha256 = sceneSha) {
      sceneVersion = version;
      sceneSha = sha256;
      sceneReady = ready;
      for (const listener of sceneListeners) listener();
    },
    snapshot,
    calls,
    routes,
    coreConflict(reason: string | null, kind: typeof editConflictKind = 'Changed') {
      editConflict = reason;
      editConflictKind = kind;
    },
    agentId: memberModel.id,
    targets: targetModel,
    targetsChanged() {
      driver.changed?.({ kind: 'change', spaceIds: [space] });
    },
    members(mode: AgentMemberMode) {
      memberModel.set(mode);
      driver.changed?.({ kind: 'change', spaceIds: [space] });
    },
    direct() {
      snapshot.conversations[0]!.kind = 'Direct';
      driver.changed?.({ kind: 'change', spaceIds: [space] });
    },
    roleLost() {
      memberModel.writable(false);
      driver.changed?.({ kind: 'change', spaceIds: [space] });
    },
    foreground(next: string) {
      appState = next;
      for (const listener of appListeners) listener(next);
    },
    async reopen(change?: 'identity' | 'device') {
      await store.close({ force: true });
      if (change === 'identity')
        snapshot.account = { ...snapshot.account!, identity_id: 'e'.repeat(64) };
      if (change === 'device')
        snapshot.account = { ...snapshot.account!, device_id: 'f'.repeat(64) };
      await store.open('account');
    },
    remote(entries: TimelineEntry[], requests: AgentRequestDto[] = []) {
      remoteEntries = entries;
      remoteRequests = requests;
      driver.changed?.({ kind: 'change', spaceIds: [space] });
    },
    errors: root.errors,
    controls,
    render: () => root.render(React.createElement(screen.MiniappScreen)),
    act: root.act,
    counters: () => ({ mounts, unmounts, flushes, presented }),
    sheets: () => sheetMounts,
    close: async () => {
      await root.close();
      previewPort?.dispose();
      await store.close({ force: true });
    },
  };
}
