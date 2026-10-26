import { t } from '../i18n/index.ts';
import { SymbolView, type SymbolViewProps } from 'expo-symbols';
import { Host, Button, Text as NativeText } from '@expo/ui';
import { Image } from 'expo-image';
import { useId, type ReactNode } from 'react';
import {
  ActivityIndicator,
  Pressable,
  Text,
  TextInput,
  View,
  type TextInputProps,
  type ViewStyle,
} from 'react-native';
import { useTheme } from '../theme';
import { contrastingInk } from '../theme/telegram';

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
  testID,
}: {
  testID?: string;
  name: SymbolViewProps['name'];
  label: string;
  onPress: () => void;
  disabled?: boolean;
}) {
  const { presentation: p } = useTheme();
  return (
    <Pressable
      testID={testID}
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
        backgroundColor: pressed ? p.list.highlighted : 'transparent',
      })}
    >
      <Icon name={name} color={p.navigation.control} />
    </Pressable>
  );
}
export function NativeButton({
  label,
  wrapLabel = false,
  onPress,
  disabled = false,
  secondary = false,
  destructive = false,
  testID,
}: {
  label: string;
  onPress: () => void;
  disabled?: boolean;
  secondary?: boolean;
  destructive?: boolean;
  testID?: string;
  /** Constrain long content labels to the parent while allowing native height to grow. */
  wrapLabel?: boolean;
}) {
  const { presentation: p, scheme } = useTheme();
  return (
    <Host
      matchContents={wrapLabel ? { vertical: true } : true}
      style={wrapLabel ? { alignSelf: 'stretch' } : undefined}
      colorScheme={scheme}
      seedColor={destructive ? p.destructive : p.accent}
    >
      <Button
        testID={testID}
        label={label}
        onPress={onPress}
        disabled={disabled}
        variant={secondary ? 'outlined' : 'filled'}
        style={wrapLabel ? { width: '100%' } : undefined}
      >
        {wrapLabel ? <NativeText textStyle={{ textAlign: 'center' }}>{label}</NativeText> : null}
      </Button>
    </Host>
  );
}
export function Field({ label, ...props }: TextInputProps & { label: string }) {
  const instanceId = useId();
  const { presentation: p } = useTheme();
  return (
    <View style={{ gap: 8 }}>
      <Text style={{ ...p.typography.footnote, color: p.list.section }}>{label}</Text>
      <TextInput
        {...props}
        testID={props.testID ?? `field-${instanceId}`}
        accessibilityLabel={label}
        placeholderTextColor={p.search.placeholder}
        style={[
          {
            minHeight: 44,
            paddingHorizontal: 16,
            paddingVertical: 10,
            fontSize: 17,
            color: p.list.text,
            backgroundColor: p.list.block,
            borderRadius: 10,
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
          cachePolicy="none"
          style={{ width: size, height: size, borderRadius: agent ? size * 0.32 : size / 2 }}
        />
      ) : (
        // The hidden monogram is artwork inside a fixed-size avatar. The
        // adjacent accessible name scales independently with Dynamic Type.
        <Text
          allowFontScaling={false}
          style={{
            fontSize: size * 0.32,
            fontWeight: '600',
            color: contrastingInk(color ?? colors.accent),
          }}
        >
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
  detail?: string;
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
      {detail ? (
        <Text
          style={{ fontSize: 16, lineHeight: 24, color: colors.secondary, textAlign: 'center' }}
        >
          {detail}
        </Text>
      ) : null}
      {children}
    </View>
  );
}
export function Loading({ label }: { label?: string }) {
  const { colors } = useTheme();
  return (
    <View
      accessible
      accessibilityRole="progressbar"
      accessibilityLabel={label ?? t('common.loading')}
      aria-busy
      style={{
        flex: 1,
        alignItems: 'center',
        justifyContent: 'center',
        gap: 12,
        backgroundColor: 'transparent',
      }}
    >
      <ActivityIndicator color={colors.accent} />
      {label ? <Text style={{ color: colors.secondary, fontSize: 17 }}>{label}</Text> : null}
    </View>
  );
}
export function Surface({ children, style }: { children: ReactNode; style?: ViewStyle }) {
  return (
    <View
      style={[
        {
          borderRadius: 24,
          borderCurve: 'continuous',
          padding: 20,
          // Content groups share the scene's sky. Inputs and actions retain
          // their own material; a group is not another opaque page layer.
          backgroundColor: 'transparent',
          gap: 12,
        },
        style,
      ]}
    >
      {children}
    </View>
  );
}
