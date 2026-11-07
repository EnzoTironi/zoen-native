import { ActivityIndicator, Pressable, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import type { useConversationMessageNavigation } from './use-conversation-message-navigation';
import { NavigationGlass } from './navigation-glass';
import { Icon } from './primitives';
import { useTheme } from '../theme';
import { t } from '../i18n/index.ts';

type ReturnNavigation = Pick<
  ReturnType<typeof useConversationMessageNavigation>,
  'canReturn' | 'returning' | 'restore' | 'current'
>;
/** Telegram's 40pt direction control, with a 44pt hit target. Its parent owns keyboard motion. */
export function ConversationReturnButton({
  navigation,
  onError,
}: {
  navigation: ReturnNavigation;
  onError(reason: unknown): void;
}) {
  const { presentation: p } = useTheme();
  const insets = useSafeAreaInsets();
  if (!navigation.canReturn) return null;
  return (
    <View
      pointerEvents="box-none"
      style={{ position: 'absolute', right: insets.right + 10, bottom: '100%', marginBottom: 6 }}
    >
      <Pressable
        testID="conversation-return-to-message"
        accessibilityRole="button"
        accessibilityLabel={t('chat.returnToMessage')}
        accessibilityState={{ disabled: navigation.returning, busy: navigation.returning }}
        disabled={navigation.returning}
        onPress={() => {
          void navigation.restore().catch((reason) => {
            if (navigation.current()) onError(reason);
          });
        }}
        style={({ pressed }) => ({
          width: 44,
          height: 44,
          borderRadius: 20,
          alignItems: 'center',
          justifyContent: 'center',
          opacity: navigation.returning ? 0.5 : pressed ? 0.65 : 1,
        })}
      >
        <NavigationGlass radius={20}>
          <View style={{ width: 40, height: 40, alignItems: 'center', justifyContent: 'center' }}>
            {navigation.returning ? (
              <ActivityIndicator color={p.navigation.control} />
            ) : (
              <Icon
                name={{
                  ios: 'chevron.down',
                  android: 'keyboard_arrow_down',
                  web: 'arrow_downward',
                }}
                size={22}
                color={p.navigation.control}
              />
            )}
          </View>
        </NavigationGlass>
      </Pressable>
    </View>
  );
}
