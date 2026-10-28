import { Pressable, Text } from 'react-native';
import Animated, {
  type SharedValue,
  useAnimatedStyle,
  useSharedValue,
} from 'react-native-reanimated';
import { contrastingInk } from '../theme/telegram';
import { Icon } from '../components/primitives';
import type { InboxAction, RowAction } from './row-actions';

/** Follow the native gesture directly; keep action ink outside the row's sky until revealed. */
export function SwipeActionPanel({
  actions,
  translation,
  edge,
  id,
  pending,
  onAction,
}: {
  actions: RowAction[];
  translation: SharedValue<number>;
  edge: 'leading' | 'trailing';
  id: string;
  pending: boolean;
  onAction(action: InboxAction): void;
}) {
  const width = useSharedValue(0);
  const style = useAnimatedStyle(() => ({
    opacity: width.get() > 0 ? 1 : 0,
    transform: [
      { translateX: translation.get() + (edge === 'leading' ? -width.get() : width.get()) },
    ],
  }));
  return (
    <Animated.View
      onLayout={(event) => width.set(event.nativeEvent.layout.width)}
      style={[{ flexDirection: 'row', alignItems: 'stretch' }, style]}
    >
      {actions.map((action) => (
        <Pressable
          key={action.id}
          testID={`inbox-${id}-${action.id}`}
          accessibilityRole="button"
          accessibilityLabel={action.label}
          disabled={pending}
          onPress={() => onAction(action.id)}
          style={({ pressed }) => ({
            minWidth: 72,
            maxWidth: 104,
            padding: 8,
            gap: 6,
            justifyContent: 'center',
            alignItems: 'center',
            backgroundColor: action.color,
            opacity: pressed || pending ? 0.6 : 1,
          })}
        >
          <Icon name={action.icon} size={24} color={contrastingInk(action.color)} />
          <Text style={{ color: contrastingInk(action.color), fontSize: 13, textAlign: 'center' }}>
            {action.label}
          </Text>
        </Pressable>
      ))}
    </Animated.View>
  );
}
