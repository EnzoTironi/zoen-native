import { Text, View } from 'react-native';
import type { useMessageDraft } from '../core/use-message-draft';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { NativeButton } from './primitives';

export type ComposerDraftState = Pick<
  ReturnType<typeof useMessageDraft>,
  'ready' | 'error' | 'rejected' | 'retry'
>;

/** The same canonical recovery state accompanies both compose and edit. */
export function ComposerDraftStatus({
  draft,
  editing,
}: {
  draft?: ComposerDraftState;
  editing: boolean;
}) {
  const { colors } = useTheme();
  if (!draft?.error) return null;
  return (
    <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 8 }}>
      <Text accessibilityRole="alert" style={{ flex: 1, color: colors.secondary, fontSize: 12 }}>
        {t(
          draft.rejected
            ? editing
              ? 'chat.editCouldNotBeSaved'
              : 'chat.draftSendRejected'
            : draft.ready
              ? 'chat.draftSaveFailed'
              : 'chat.draftLoadFailed',
        )}
      </Text>
      <NativeButton
        testID="retry-message-draft"
        label={t('chat.tryAgain2')}
        secondary
        onPress={draft.retry}
      />
    </View>
  );
}
