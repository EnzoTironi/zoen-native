import { View, Text } from 'react-native';
import { SettingsScreen } from '../components/settings-screen';
import { EmptyState, Loading, NativeButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useSavedMessages } from '../saved-messages/use-saved-messages';
import { useTheme } from '../theme';

export default function SavedMessages() {
  const { retry, phase } = useSavedMessages();
  const { colors } = useTheme();
  return (
    <SettingsScreen title={t('settings.savedMessages')} testID="saved-messages-entry">
      {phase === 'opening' ? (
        <View style={{ minHeight: 280 }}>
          <Loading label={t('settings.savedMessagesOpening')} />
        </View>
      ) : (
        <EmptyState
          title={t('settings.savedMessages')}
          icon={{ ios: 'bookmark', android: 'bookmark', web: 'bookmark' }}
        >
          <Text
            selectable
            accessibilityRole={phase === 'failed' ? 'alert' : undefined}
            style={{ color: colors.secondary, fontSize: 16, textAlign: 'center' }}
          >
            {t(
              phase === 'unavailable'
                ? 'settings.savedMessagesAccountRequired'
                : phase === 'failed'
                  ? 'settings.savedMessagesFailed'
                  : 'settings.savedMessagesWaiting',
            )}
          </Text>
          {phase !== 'unavailable' ? (
            <NativeButton
              testID="saved-messages-retry"
              label={t('settings.tryAgain')}
              onPress={() => {
                void retry();
              }}
            />
          ) : null}
        </EmptyState>
      )}
    </SettingsScreen>
  );
}
