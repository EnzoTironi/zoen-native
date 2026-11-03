import { Text, View } from 'react-native';
import { Host, Button } from '@expo/ui';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { IconButton } from './primitives';

/** Contextual actions; selection and authorization remain caller-owned. */
export function MessageSelectionToolbar({
  count,
  onCopy,
  onForward,
  canCopy,
  canForward,
  canDelete,
  onDelete,
  pending,
}: {
  count: number;
  onCopy: () => void;
  onForward: () => void;
  canCopy: boolean;
  canForward: boolean;
  canDelete: boolean;
  onDelete: () => void;
  pending: boolean;
}) {
  const { presentation: p } = useTheme();
  return (
    <View
      testID="message-selection-toolbar"
      style={{
        minHeight: 50,
        paddingHorizontal: 8,
        flexDirection: 'row',
        alignItems: 'center',
        justifyContent: 'space-between',
        backgroundColor: p.navigation.background,
      }}
    >
      <IconButton
        testID="message-selection-delete"
        name="trash"
        label={t('chat.deleteSelectedMessages', { count })}
        disabled={pending || !count || !canDelete}
        onPress={onDelete}
      />
      <IconButton
        testID="message-selection-copy"
        name="doc.on.doc"
        label={t('chat.copyText')}
        disabled={pending || !count || !canCopy}
        onPress={onCopy}
      />
      <IconButton
        testID="message-selection-forward"
        name="arrowshape.turn.up.right"
        label={t('chat.forward')}
        disabled={pending || !count || !canForward}
        onPress={onForward}
      />
    </View>
  );
}

/** Native navigation title with an accessible selection cancellation action. */
export function MessageSelectionHeader({
  count,
  onCancel,
}: {
  count: number;
  onCancel: () => void;
}) {
  const { presentation: p } = useTheme();
  return (
    <Text
      testID="message-selection-count"
      accessibilityRole="header"
      accessibilityLiveRegion="polite"
      accessibilityLabel={t('chat.selectedMessages', { count })}
      accessibilityActions={[{ name: 'cancel', label: t('chat.cancel') }]}
      onAccessibilityAction={({ nativeEvent }) => {
        if (nativeEvent.actionName === 'cancel') onCancel();
      }}
      style={{ color: p.navigation.text, fontSize: 17, fontWeight: '600', textAlign: 'center' }}
    >
      {count}
    </Text>
  );
}

export function MessageSelectionCancel({ onCancel }: { onCancel: () => void }) {
  const { presentation: p, scheme } = useTheme();
  return (
    <Host matchContents colorScheme={scheme} seedColor={p.navigation.control}>
      <Button
        testID="message-selection-cancel"
        label={t('chat.cancel')}
        variant="text"
        onPress={onCancel}
      />
    </Host>
  );
}
