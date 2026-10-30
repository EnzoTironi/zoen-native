import { messageMenuActions } from './message-actions-model';
import { Modal, Pressable, ScrollView, Text, View } from 'react-native';
import { NativeButton } from './primitives';
import { ReactionPalette } from './reaction-palette';
import { useTheme } from '../theme';
import { t } from '../i18n/index.ts';
import type { MessageActionsProps } from './message-actions.types';
import { useEffect, useRef } from 'react';

/** Portable fallback; iPhone uses the anchored native presentation and dismissal completion. */
export function MessageActions(props: MessageActionsProps) {
  const { colors } = useTheme();
  const entry = props.entry;
  const onClose = useRef(props.onClose);
  useEffect(() => {
    onClose.current = props.onClose;
  }, [props.onClose]);
  const custom = Boolean(props.options);
  useEffect(() => {
    if (custom && !entry) onClose.current();
  }, [custom, entry]);
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
        {entry && !props.options && !entry?.deleted ? (
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
        {props.error ? (
          <Text accessibilityRole="alert" style={{ color: colors.danger }}>
            {props.error}
          </Text>
        ) : null}
        {props.options ? (
          <>
            <ScrollView style={{ maxHeight: '55%' }}>{props.preview}</ScrollView>
            <View style={{ backgroundColor: colors.surface, padding: 12, borderRadius: 20 }}>
              {props.options.map((row) => (
                <NativeButton
                  key={row.label}
                  label={row.label}
                  destructive={row.destructive}
                  disabled={props.pending || row.disabled}
                  onPress={() => {
                    if (props.pending || row.disabled) return;
                    if (!row.keepOpen) props.onClose();
                    row.run();
                  }}
                />
              ))}
            </View>
          </>
        ) : props.mode === 'actions' ? (
          <View style={{ backgroundColor: colors.surface, padding: 12, borderRadius: 20, gap: 6 }}>
            {messageMenuActions(props).map((action) => (
              <NativeButton
                key={action.id}
                secondary={!action.destructive}
                destructive={action.destructive}
                label={action.label}
                disabled={props.pending}
                onPress={() => run(action.run)}
              />
            ))}
          </View>
        ) : null}
      </View>
    </Modal>
  );
}
