import { SymbolView, type SymbolViewProps } from 'expo-symbols';
import { Host, Button } from '@expo/ui';
import type { ReactNode } from 'react';
import {
  ActivityIndicator,
  Image,
  Pressable,
  Text,
  TextInput,
  View,
  type TextInputProps,
  type ViewStyle,
} from 'react-native';
import { useTheme } from '../theme';

export function Icon({
  name,
  size = 22,
  color,
}: {
  name: SymbolViewProps['name'];
  size?: number;
  color?: string;
}) {
  const { colors } = useTheme();
  return (
    <SymbolView
      name={name}
      size={size}
      tintColor={color ?? colors.text}
      style={{ width: size, height: size }}
    />
  );
}
export function IconButton({
  name,
  label,
  onPress,
  disabled = false,
}: {
  name: SymbolViewProps['name'];
  label: string;
  onPress: () => void;
  disabled?: boolean;
}) {
  const { colors } = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => ({
        minWidth: 44,
        minHeight: 44,
        alignItems: 'center',
        justifyContent: 'center',
        opacity: disabled ? 0.35 : pressed ? 0.5 : 1,
        borderRadius: 22,
        backgroundColor: pressed ? colors.muted : 'transparent',
      })}
    >
      <Icon name={name} />
    </Pressable>
  );
}
export function NativeButton({
  label,
  onPress,
  disabled = false,
  secondary = false,
}: {
  label: string;
  onPress: () => void;
  disabled?: boolean;
  secondary?: boolean;
}) {
  const { colors, scheme } = useTheme();
  return (
    <Host matchContents colorScheme={scheme} seedColor={colors.accent}>
      <Button
        label={label}
        onPress={onPress}
        disabled={disabled}
        variant={secondary ? 'outlined' : 'filled'}
      />
    </Host>
  );
}
export function Field({ label, ...props }: TextInputProps & { label: string }) {
  const { colors } = useTheme();
  return (
    <View style={{ gap: 8 }}>
      <Text style={{ fontSize: 14, fontWeight: '600', color: colors.secondary }}>{label}</Text>
      <TextInput
        {...props}
        accessibilityLabel={label}
        placeholderTextColor={colors.tertiary}
        style={[
          {
            minHeight: 52,
            paddingHorizontal: 16,
            paddingVertical: 14,
            fontSize: 17,
            color: colors.text,
            backgroundColor: colors.muted,
            borderRadius: 16,
            borderCurve: 'continuous',
          },
          props.style,
        ]}
      />
    </View>
  );
}
export function Avatar({
  name,
  color,
  size = 48,
  agent = false,
  uri,
}: {
  name: string;
  color?: string;
  size?: number;
  agent?: boolean;
  uri?: string;
}) {
  const { colors } = useTheme();
  return (
    <View
      accessibilityElementsHidden
      style={{
        width: size,
        height: size,
        borderRadius: agent ? size * 0.32 : size / 2,
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: color ?? colors.accent,
      }}
    >
      {uri ? (
        <Image
          source={{ uri }}
          style={{ width: size, height: size, borderRadius: agent ? size * 0.32 : size / 2 }}
        />
      ) : (
        <Text style={{ fontSize: size * 0.32, fontWeight: '600', color: '#FFFFFF' }}>
          {name
            .split(/\s+/)
            .slice(0, 2)
            .map((part) => part[0])
            .join('')
            .toUpperCase()}
        </Text>
      )}
    </View>
  );
}
export function EmptyState({
  title,
  detail,
  icon = { ios: 'bubble.left.and.bubble.right', android: 'chat', web: 'chat' },
  children,
}: {
  title: string;
  detail: string;
  icon?: SymbolViewProps['name'];
  children?: ReactNode;
}) {
  const { colors } = useTheme();
  return (
    <View
      style={{
        flex: 1,
        minHeight: 280,
        maxWidth: 440,
        alignSelf: 'center',
        alignItems: 'center',
        justifyContent: 'center',
        padding: 32,
        gap: 16,
      }}
    >
      <View style={{ padding: 22, backgroundColor: colors.muted, borderRadius: 28 }}>
        <Icon name={icon} size={32} color={colors.accent} />
      </View>
      <Text
        accessibilityRole="header"
        style={{ fontSize: 24, fontWeight: '600', color: colors.text, textAlign: 'center' }}
      >
        {title}
      </Text>
      <Text style={{ fontSize: 16, lineHeight: 24, color: colors.secondary, textAlign: 'center' }}>
        {detail}
      </Text>
      {children}
    </View>
  );
}
export function Loading() {
  const { colors } = useTheme();
  return (
    <View
      accessibilityLabel="Carregando"
      style={{
        flex: 1,
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: colors.background,
      }}
    >
      <ActivityIndicator color={colors.accent} />
    </View>
  );
}
export function Surface({ children, style }: { children: ReactNode; style?: ViewStyle }) {
  const { colors } = useTheme();
  return (
    <View
      style={[
        {
          borderRadius: 24,
          borderCurve: 'continuous',
          padding: 20,
          backgroundColor: colors.surface,
          gap: 12,
        },
        style,
      ]}
    >
      {children}
    </View>
  );
}
