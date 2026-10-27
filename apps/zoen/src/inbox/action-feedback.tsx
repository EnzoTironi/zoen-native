import { Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { IconButton, NativeButton } from '../components/primitives';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { InboxActionsSession, InboxState, InboxTicket } from './actions-session';

export function InboxActionFeedback({
  state,
  session,
  ticket,
}: {
  state: InboxState;
  session: InboxActionsSession;
  ticket: InboxTicket | null;
}) {
  const { presentation: p } = useTheme(),
    insets = useSafeAreaInsets();
  const undo = state.undo;
  if (!undo && !state.error) return null;
  const message = state.error
    ? t(state.error === 'undo' ? 'inboxActions.undoNotConfirmed' : 'inboxActions.changed')
    : t(
        undo!.receipt.field === 'archived'
          ? undo!.receipt.value
            ? 'inboxActions.archived'
            : 'inboxActions.unarchived'
          : undo!.receipt.value
            ? 'inboxActions.pinned'
            : 'inboxActions.unpinned',
        { title: undo!.title },
      );
  return (
    <View
      testID="inbox-action-feedback"
      style={{
        padding: 12,
        paddingBottom: Math.max(12, insets.bottom),
        flexDirection: 'row',
        alignItems: 'center',
        gap: 8,
        backgroundColor: p.list.block,
      }}
    >
      <Text
        selectable
        accessibilityRole={state.error ? 'alert' : 'text'}
        accessibilityLiveRegion="polite"
        style={{ flex: 1, color: state.error ? p.destructive : p.list.text, fontSize: 15 }}
      >
        {message}
      </Text>
      {undo && !state.error ? (
        <NativeButton
          secondary
          testID="inbox-undo"
          label={t('inboxActions.undo')}
          disabled={state.pending !== null}
          onPress={() => {
            void session.undo(undo, ticket);
          }}
        />
      ) : null}
      <IconButton
        label={t('common.close')}
        name={{ ios: 'xmark', android: 'close' }}
        onPress={() => session.dismiss(ticket)}
      />
    </View>
  );
}
