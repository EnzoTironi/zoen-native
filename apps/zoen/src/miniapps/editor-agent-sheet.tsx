import * as Crypto from 'expo-crypto';
import { BottomSheet, Host, RNHostView } from '@expo/ui';
import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { AppState, Keyboard, View, useWindowDimensions } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { IconButton } from '../components/primitives';
import { useMemberActions } from '../communities/use-member-actions';
import { MemberAdditionContent } from '../communities/member-addition-sheet';
import { useClient } from '../core/provider';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { useAppSheetCloseButton } from './app-sheet-context';
import type { MiniappDocumentController } from './document-controller';
import { editorAgentPort, editorAgentScope } from './editor-agent-port';
import { EditorAgentSession } from './editor-agent-session';
import { EditorAgentContent } from './editor-agent-content';
import { EditorAgentEnrollment } from './editor-agent-enrollment';
import { editorAgentAdmission } from './editor-agent-admission';

const readyFallback = () => true;
const noReadinessChanges = () => () => {};
type Pane = 'prompt' | 'members' | 'enrollment';
/** State-presented content changes never replace the sibling document or its session. */
export function EditorAgentAccess({
  itemId,
  spaceId,
  document,
}: {
  itemId: string;
  spaceId: string;
  document: MiniappDocumentController;
}) {
  const ready = useSyncExternalStore(
    document.agentReadiness?.subscribe ?? noReadinessChanges,
    document.agentReadiness?.getSnapshot ?? readyFallback,
    document.agentReadiness?.getSnapshot ?? readyFallback,
  );
  const client = useClient();
  const { store } = client;
  const { scheme, colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const chrome = useAppSheetCloseButton();
  const owner = editorAgentScope(store, spaceId, client);
  const [pane, setPane] = useState<Pane>('prompt');
  const [enrollmentOpened, setEnrollmentOpened] = useState(false);
  const members = useMemberActions(spaceId, true);
  const session = useMemo(
    () =>
      new EditorAgentSession(
        editorAgentPort(store, itemId, spaceId, document, owner, () => Crypto.randomUUID()),
        owner,
        itemId,
        spaceId,
      ),
    [store, itemId, spaceId, document, owner],
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const ticket = session.ticket(state);
  const memberTicket = members.addition.ticket(members.additionState);
  const admission = ready ? editorAgentAdmission(members.additionState) : 'loading';
  const navigate = (next: Pane) => {
    if (!session.accepts(ticket)) return;
    Keyboard.dismiss();
    session.renewTicket();
    if (next === 'members') {
      members.addition.open(memberTicket, true);
    }
    if (next !== 'members') {
      members.addition.close(memberTicket, true);
      void members.addition.refresh();
    }
    if (next === 'enrollment') setEnrollmentOpened(true);
    setPane(next);
  };
  const close = () => {
    if (!session.accepts(ticket)) return;
    members.addition.close(memberTicket, true);
    session.close();
  };
  useEffect(() => {
    session.activate();
    const stop = store.subscribe(() => {
      if (editorAgentScope(store, spaceId) !== owner) session.retire();
    });
    const changed = store.subscribeCanonicalChanges(() => {
      void session.refresh();
    });
    const app = AppState.addEventListener('change', (next) => {
      if (next !== 'active') session.close();
    });
    return () => {
      stop();
      changed();
      app.remove();
      session.retire();
    };
  }, [store, spaceId, owner, session]);
  const visiblePane = state.visible ? pane : 'prompt';
  return (
    <>
      {ready ? (
        <View
          style={{
            position: 'absolute',
            top: chrome?.top ?? insets.top + 8,
            right: (chrome?.right ?? insets.right + 12) + (chrome?.width ?? 44) + 8,
            borderRadius: 24,
            backgroundColor: colors.surface,
          }}
        >
          <IconButton
            testID="editor-agent-open"
            name={{ ios: 'sparkles', android: 'auto_awesome', web: 'auto_awesome' }}
            label={t('miniapps.askAgent')}
            disabled={!owner || state.visible}
            onPress={() => {
              setPane('prompt');
              session.open();
              void members.addition.refresh();
            }}
          />
        </View>
      ) : null}
      <Host
        style={{ position: 'absolute' }}
        pointerEvents="none"
        colorScheme={scheme}
        seedColor={colors.accent}
      >
        <BottomSheet
          testID="editor-agent-sheet"
          isPresented={state.visible}
          onDismiss={close}
          snapPoints={['half', 'full']}
          containerColor={colors.background}
        >
          <RNHostView>
            <View style={{ flex: 1 }}>
              <View
                style={{ flex: 1, display: visiblePane === 'prompt' ? 'flex' : 'none' }}
                accessibilityElementsHidden={visiblePane !== 'prompt'}
                importantForAccessibility={
                  visiblePane === 'prompt' ? 'auto' : 'no-hide-descendants'
                }
              >
                <EditorAgentContent
                  session={session}
                  state={state}
                  ticket={ticket}
                  itemId={itemId}
                  width={width}
                  admission={admission}
                  hasAgents={members.additionState.allResults.length > 0}
                  onClose={() => {
                    if (session.accepts(ticket)) close();
                  }}
                  onAdd={() =>
                    navigate(members.additionState.allResults.length ? 'members' : 'enrollment')
                  }
                  onRefresh={() => {
                    if (session.accepts(ticket)) void members.addition.refresh();
                  }}
                  onSend={() => {
                    if (
                      !ready ||
                      !session.accepts(ticket) ||
                      (!session.getSnapshot().uncertain &&
                        editorAgentAdmission(members.addition.getSnapshot()) !== 'ready')
                    )
                      return;
                    void session.send(ticket).then(() => {
                      if (
                        session.getSnapshot().conflictKind === 'MissingOwnedAgent' ||
                        session.getSnapshot().conflictKind === 'WaitingForMls'
                      )
                        void members.addition.refresh();
                    });
                  }}
                />
              </View>
              <View
                style={{ flex: 1, display: visiblePane === 'members' ? 'flex' : 'none' }}
                accessibilityElementsHidden={visiblePane !== 'members'}
                importantForAccessibility={
                  visiblePane === 'members' ? 'auto' : 'no-hide-descendants'
                }
              >
                <MemberAdditionContent
                  session={members.addition}
                  state={members.additionState}
                  inlineConfirmation
                  title={t('miniapps.addOwnAgent')}
                  onBack={() => navigate('prompt')}
                  onClose={() => {
                    if (session.accepts(ticket)) close();
                  }}
                />
              </View>
              {enrollmentOpened ? (
                <View
                  style={{ flex: 1, display: visiblePane === 'enrollment' ? 'flex' : 'none' }}
                  accessibilityElementsHidden={visiblePane !== 'enrollment'}
                  importantForAccessibility={
                    visiblePane === 'enrollment' ? 'auto' : 'no-hide-descendants'
                  }
                >
                  <EditorAgentEnrollment
                    key={owner}
                    spaceId={spaceId}
                    active={state.visible && visiblePane === 'enrollment'}
                    allowed={() => session.accepts(ticket) && pane === 'enrollment'}
                    onBack={() => navigate('prompt')}
                    onContinue={() => {
                      navigate('members');
                      void members.addition.refresh();
                    }}
                    onClose={close}
                  />
                </View>
              ) : null}
            </View>
          </RNHostView>
        </BottomSheet>
      </Host>
    </>
  );
}
