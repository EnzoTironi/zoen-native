import { Pressable, View } from 'react-native';
import { Icon } from '../components/primitives';
import { t } from '../i18n';
import { APP_CARD_METRICS } from './app-card-layout';

/** Sibling native hit targets; selecting the cover never opens a nested menu. */
export function AppCardEditActions({
  id,
  name,
  pinned,
  disabled,
  onRemove,
  onTogglePin,
}: {
  id: string;
  name: string;
  pinned: boolean;
  disabled: boolean;
  onRemove?: () => void;
  onTogglePin?: () => void;
}) {
  const button = (kind: 'remove' | 'pin', onPress: (() => void) | undefined) =>
    onPress ? (
      <Pressable
        testID={`app-card-${kind}-${id}`}
        accessibilityRole="button"
        accessibilityLabel={
          kind === 'remove'
            ? t('home.removeShortcutNamed', { name })
            : t(pinned ? 'home.unpinNamed' : 'home.pinNamed', { name })
        }
        accessibilityState={{ disabled, ...(kind === 'pin' ? { selected: pinned } : {}) }}
        disabled={disabled}
        onPress={onPress}
        style={({ pressed }) => ({
          width: APP_CARD_METRICS.menuSize,
          height: APP_CARD_METRICS.menuSize,
          alignItems: 'center',
          justifyContent: 'center',
          borderRadius: APP_CARD_METRICS.menuSize / 2,
          backgroundColor: '#FFFFFFE8',
          opacity: disabled ? 0.4 : pressed ? 0.6 : 1,
        })}
      >
        <Icon
          name={
            kind === 'remove'
              ? { ios: 'xmark', android: 'close', web: 'close' }
              : pinned
                ? { ios: 'pin.fill', android: 'push_pin', web: 'push_pin' }
                : { ios: 'pin', android: 'push_pin', web: 'push_pin' }
          }
          size={20}
          color="#202733"
        />
      </Pressable>
    ) : null;
  return (
    <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
      {button('remove', onRemove)}
      {button('pin', onTogglePin)}
    </View>
  );
}
