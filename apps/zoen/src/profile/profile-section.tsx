import type { ReactNode } from 'react';
import { Pressable, Text, View } from 'react-native';
import { useTheme } from '../theme';
import { Icon } from '../components/primitives';
import { peerInfo } from './telegram-profile-presentation';
export function ProfileSection({
  title,
  children,
  footer,
}: {
  title?: string;
  children: ReactNode;
  footer?: string;
}) {
  const { presentation: p } = useTheme();
  return (
    <View style={{ gap: 7 }}>
      {title ? (
        <Text
          accessibilityRole="header"
          style={{ color: p.list.section, fontSize: 13, paddingHorizontal: 16 }}
        >
          {title}
        </Text>
      ) : null}
      <View
        style={{ borderRadius: peerInfo.corner, overflow: 'hidden', backgroundColor: p.list.block }}
      >
        {children}
      </View>
      {footer ? (
        <Text
          style={{ color: p.list.secondary, fontSize: 13, lineHeight: 18, paddingHorizontal: 16 }}
        >
          {footer}
        </Text>
      ) : null}
    </View>
  );
}
export function ProfileInfoRow({
  label,
  value,
  onPress,
  destructive,
  disabled,
  testID,
}: {
  label: string;
  value?: string;
  onPress?: () => void;
  destructive?: boolean;
  disabled?: boolean;
  testID?: string;
}) {
  const { presentation: p } = useTheme();
  const content = (
    <>
      <View style={{ flex: 1, gap: value ? 3 : 0 }}>
        <Text
          selectable={!onPress}
          style={{
            color: destructive ? p.destructive : value ? p.list.accent : p.list.text,
            fontSize: value ? 13 : 17,
          }}
        >
          {label}
        </Text>
        {value ? (
          <Text selectable={!onPress} style={{ color: p.list.text, fontSize: 17, lineHeight: 22 }}>
            {value}
          </Text>
        ) : null}
      </View>
      {onPress ? (
        <Icon
          name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
          size={13}
          color={p.list.disclosure}
        />
      ) : null}
    </>
  );
  const style = {
    minHeight: peerInfo.row,
    paddingHorizontal: peerInfo.edge,
    paddingVertical: 12,
    flexDirection: 'row' as const,
    alignItems: 'center' as const,
    gap: 12,
  };
  return onPress ? (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={value ? `${label}, ${value}` : label}
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => ({
        ...style,
        opacity: disabled ? 0.4 : 1,
        backgroundColor: pressed ? p.list.highlighted : 'transparent',
      })}
    >
      {content}
    </Pressable>
  ) : (
    <View testID={testID} style={style}>
      {content}
    </View>
  );
}
