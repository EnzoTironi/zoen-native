import { Text, View } from 'react-native';
import { t } from '../i18n/index.ts';
import { messageText } from '../core/presentation';
import type { TimelineEntry } from '../core/types';
import { useTheme } from '../theme';
import { Icon, IconButton } from './primitives';

/** The quote belongs to the composer, so keyboard and quote always move together. */
export function MessageContext({
  entry,
  editing = false,
  onCancel,
}: {
  entry: TimelineEntry;
  editing?: boolean;
  onCancel?: () => void;
}) {
  const { colors } = useTheme();
  return (
    <View
      testID={editing ? 'editing-message-context' : 'reply-message-context'}
      style={{
        marginHorizontal: 20,
        paddingLeft: 10,
        borderLeftWidth: 3,
        borderLeftColor: colors.accent,
        flexDirection: 'row',
        alignItems: 'center',
        gap: 10,
      }}
    >
      <Icon
        name={
          editing
            ? { ios: 'pencil', android: 'edit', web: 'edit' }
            : { ios: 'arrowshape.turn.up.left', android: 'reply', web: 'reply' }
        }
        size={18}
        color={colors.secondary}
      />
      <View style={{ flex: 1, gap: 3, paddingVertical: 7 }}>
        <Text numberOfLines={1} style={{ color: colors.text, fontSize: 13, fontWeight: '600' }}>
          {editing ? t('chat.editMessage') : t('chat.replyingTo', { name: entry.author.name })}
        </Text>
        <Text numberOfLines={1} style={{ color: colors.secondary, fontSize: 13, lineHeight: 18 }}>
          {entry.voice ? t('media.voiceMessage') : messageText(entry)}
        </Text>
      </View>
      {onCancel ? (
        <IconButton
          label={t('chat.cancel')}
          name={{ ios: 'xmark', android: 'close', web: 'close' }}
          onPress={onCancel}
        />
      ) : null}
    </View>
  );
}
