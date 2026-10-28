import { Text, View } from 'react-native';
import { Host, Button } from '@expo/ui';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { IconButton } from './primitives';

/** Contextual actions; selection and authorization remain caller-owned. */
export function MessageSelectionToolbar({
  count,
  onCancel,
  onCopy,
  onForward,
  canCopy,
  canForward,
}: {
  count: number;
  onCancel: () => void;
  onCopy: () => void;
  onForward: () => void;
  canCopy: boolean;
  canForward: boolean;
}) {
  const { presentation: p, scheme } = useTheme();
  return (
    <View
      testID="message-selection-toolbar"
      style={{
        minHeight: 50,
        paddingHorizontal: 8,
        flexDirection: 'row',
        alignItems: 'center',
        backgroundColor: p.navigation.background,
      }}
    >
      <Host matchContents colorScheme={scheme} seedColor={p.navigation.control}>
        <Button
          testID="message-selection-cancel"
          label={t('chat.cancel')}
          variant="text"
          onPress={onCancel}
        />
      </Host>
      <Text
        accessibilityLiveRegion="polite"
        accessibilityLabel={t('chat.selectedMessages', { count })}
        style={{
          flex: 1,
          textAlign: 'center',
          color: p.navigation.text,
          fontSize: 17,
          fontWeight: '600',
        }}
      >
        {count}
      </Text>
      <IconButton
        testID="message-selection-copy"
        name="doc.on.doc"
        label={t('chat.copyText')}
        disabled={!count || !canCopy}
        onPress={onCopy}
      />
      <IconButton
        testID="message-selection-forward"
        name="arrowshape.turn.up.right"
        label={t('chat.forward')}
        disabled={!count || !canForward}
        onPress={onForward}
      />
    </View>
  );
}
