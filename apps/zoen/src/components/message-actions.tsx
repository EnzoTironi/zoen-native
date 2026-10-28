import { Modal, Pressable, View } from 'react-native';
import { NativeButton } from './primitives';
import { ReactionPalette } from './reaction-palette';
import { useTheme } from '../theme';
import { t } from '../i18n/index.ts';
import type { MessageActionsProps } from './message-actions.types';

/** Portable fallback; iPhone uses the anchored native presentation and dismissal completion. */
export function MessageActions(props: MessageActionsProps) {
  const { colors } = useTheme();
  const entry = props.entry;
  function run(callback?: MessageActionsProps['onReply']) {
    if (entry && !props.pending && callback) {
      props.onClose();
      callback(entry);
    }
  }
  return (
    <Modal transparent visible={Boolean(entry)} animationType="fade" onRequestClose={props.onClose}>
      <View
        accessibilityViewIsModal
        style={{ flex: 1, justifyContent: 'center', padding: 16, gap: 10 }}
      >
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={t('chat.closeMessageActions')}
          onPress={props.onClose}
          style={{ position: 'absolute', inset: 0, backgroundColor: 'rgba(0,0,0,0.28)' }}
        />
        {entry ? (
          <ReactionPalette
            reactions={entry.reactions}
            pending={props.pending}
            onReact={(emoji) => {
              if (!props.pending) {
                props.onReact(emoji);
              }
            }}
            onMore={props.onMore}
          />
        ) : null}
        {props.mode === 'actions' ? (
          <View style={{ backgroundColor: colors.surface, padding: 12, borderRadius: 20, gap: 6 }}>
            {props.onRecover &&
            entry?.author.is_me &&
            !entry.deleted &&
            entry.delivery === 'Failed' ? (
              <NativeButton
                secondary
                label={t('media.prepareNewSend')}
                disabled={props.pending}
                onPress={() => run(props.onRecover)}
              />
            ) : null}
            <NativeButton
              secondary
              label={t('chat.reply')}
              disabled={props.pending}
              onPress={() => run(props.onReply)}
            />
            <NativeButton
              secondary
              label={t('chat.openReplies')}
              disabled={props.pending}
              onPress={() => run(props.onThread)}
            />
            {props.onForward ? (
              <NativeButton
                secondary
                label={t('chat.forward')}
                disabled={props.pending}
                onPress={() => run(props.onForward)}
              />
            ) : null}
            {props.onInfo ? (
              <NativeButton
                secondary
                label={t('messageInfo.title')}
                disabled={props.pending}
                onPress={() => run(props.onInfo)}
              />
            ) : null}
            {props.onSelect ? (
              <NativeButton
                secondary
                label={t('chat.selectMessages')}
                disabled={props.pending}
                onPress={() => run(props.onSelect)}
              />
            ) : null}
            {props.onCopy ? (
              <NativeButton
                secondary
                label={t('chat.copyText')}
                disabled={props.pending}
                onPress={() => run(props.onCopy)}
              />
            ) : null}
            {entry?.author.is_me && props.onEdit ? (
              <NativeButton
                secondary
                label={t('chat.edit')}
                disabled={props.pending}
                onPress={() => run(props.onEdit)}
              />
            ) : null}
            {entry?.author.is_me && props.onDelete ? (
              <NativeButton
                destructive
                label={t('chat.deleteForEveryone')}
                disabled={props.pending}
                onPress={() => run(props.onDelete)}
              />
            ) : null}
          </View>
        ) : null}
      </View>
    </Modal>
  );
}
