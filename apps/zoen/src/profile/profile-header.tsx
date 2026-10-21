import type { ComponentProps, ReactNode } from 'react';
import { Pressable, Text, View } from 'react-native';
import { Avatar, Icon } from '../components/primitives';
import { useTheme } from '../theme';
import { peerInfo } from './telegram-profile-presentation';

export type ProfileHeaderAction = Readonly<{
  id: string;
  label: string;
  icon: ComponentProps<typeof Icon>['name'];
  onPress: () => void;
  disabled?: boolean;
}>;
/** The native stack owns Back and its gestures; this is the scrolling profile content only. */
export function ProfileHeader({
  name,
  subtitle,
  photo,
  tint,
  agent = false,
  top = 0,
  actions = [],
  children,
  onAvatarPress,
  avatarLabel,
}: {
  name: string;
  subtitle?: string;
  photo?: string;
  tint?: string;
  agent?: boolean;
  top?: number;
  actions?: readonly ProfileHeaderAction[];
  children?: ReactNode;
  onAvatarPress?: () => void;
  avatarLabel?: string;
}) {
  const { presentation: p } = useTheme();
  const avatar = (
    <Avatar uri={photo} name={name} color={tint} agent={agent} size={peerInfo.avatar} />
  );
  return (
    <View testID="peer-info-header" style={{ alignItems: 'center', paddingTop: top }}>
      {onAvatarPress ? (
        <Pressable
          testID="peer-info-avatar"
          accessibilityRole="button"
          accessibilityLabel={avatarLabel ?? name}
          onPress={onAvatarPress}
          style={({ pressed }) => ({ opacity: pressed ? 0.7 : 1 })}
        >
          {avatar}
        </Pressable>
      ) : (
        avatar
      )}
      <Text
        testID="peer-info-title"
        selectable
        accessibilityRole="header"
        style={{
          color: p.text,
          fontSize: peerInfo.titleFont,
          lineHeight: peerInfo.titleLine,
          fontWeight: '500',
          textAlign: 'center',
          marginTop: peerInfo.titleGap + (subtitle ? 0 : 11),
        }}
      >
        {name}
      </Text>
      {subtitle ? (
        <Text
          selectable
          style={{
            color: p.secondary,
            fontSize: peerInfo.subtitleFont,
            lineHeight: peerInfo.subtitleLine,
            textAlign: 'center',
            marginTop: peerInfo.subtitleGap,
          }}
        >
          {subtitle}
        </Text>
      ) : null}
      {children}
      {actions.length ? (
        <View
          testID="peer-info-actions"
          style={{ flexDirection: 'row', width: '100%', gap: peerInfo.actionGap, marginTop: 16 }}
        >
          {actions.map((action) => (
            <Pressable
              key={action.id}
              testID={action.id}
              accessibilityRole="button"
              accessibilityLabel={action.label}
              accessibilityState={{ disabled: action.disabled }}
              disabled={action.disabled}
              onPress={action.onPress}
              style={({ pressed }) => ({
                flex: 1,
                minWidth: 0,
                height: peerInfo.actionHeight,
                borderRadius: peerInfo.corner,
                alignItems: 'center',
                backgroundColor: pressed ? p.list.highlighted : p.list.block,
                opacity: action.disabled ? 0.4 : 1,
              })}
            >
              <View
                style={{
                  height: peerInfo.actionIconFrame,
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                <Icon name={action.icon} size={26} color={p.list.accent} />
              </View>
              <Text
                numberOfLines={1}
                style={{
                  color: p.list.accent,
                  fontSize: peerInfo.actionFont,
                  lineHeight: 13,
                  textAlign: 'center',
                }}
              >
                {action.label.toLocaleLowerCase()}
              </Text>
            </Pressable>
          ))}
        </View>
      ) : null}
    </View>
  );
}
