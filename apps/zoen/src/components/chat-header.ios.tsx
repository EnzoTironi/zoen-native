import { Stack } from 'expo-router';
import { Host } from '@expo/ui';
import { Button, HStack } from '@expo/ui/swift-ui';
import {
  buttonStyle,
  disabled,
  font,
  foregroundStyle,
  frame,
  labelStyle,
  opacity,
} from '@expo/ui/swift-ui/modifiers';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { ChatTitle, type ChatHeaderProps } from './chat-title';

export function ChatHeader(props: ChatHeaderProps) {
  const { colors, scheme } = useTheme();
  const search = props.search;
  return (
    <>
      <Stack.Screen options={{ title: props.title, headerBackVisible: true }} />
      <Stack.Title asChild>
        <ChatTitle {...props} />
      </Stack.Title>
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.View>
          <Host style={{ width: 132, height: 44 }} colorScheme={scheme}>
            <HStack spacing={0}>
              {(
                [
                  { icon: 'phone', label: t('chat.voiceCallUnavailable'), unavailable: true },
                  { icon: 'video', label: t('chat.videoCallUnavailable'), unavailable: true },
                  {
                    icon: 'magnifyingglass',
                    label: t(search?.active ? 'chat.closeSearch' : 'chat.searchThisChat'),
                    unavailable: false,
                  },
                ] as const
              ).map(({ icon, label, unavailable }) => (
                <Button
                  key={icon}
                  systemImage={icon}
                  label={label}
                  onPress={unavailable ? undefined : search?.onToggle}
                  modifiers={[
                    buttonStyle('plain'),
                    labelStyle('iconOnly'),
                    font({ size: 19 }),
                    foregroundStyle(!unavailable && search?.active ? colors.accent : colors.text),
                    disabled(unavailable),
                    opacity(unavailable ? 0.35 : 1),
                    frame({ width: 44, height: 44 }),
                  ]}
                />
              ))}
            </HStack>
          </Host>
        </Stack.Toolbar.View>
      </Stack.Toolbar>
    </>
  );
}
