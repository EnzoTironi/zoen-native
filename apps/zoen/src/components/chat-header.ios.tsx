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
import { ConversationSearch } from './conversation-search';

export function ChatHeader(props: ChatHeaderProps) {
  const { colors, scheme } = useTheme();
  const search = props.search;
  if (search?.state.active)
    return (
      <>
        <Stack.Screen options={{ title: props.title, headerBackVisible: false }} />
        <Stack.Title asChild>
          <ConversationSearch {...search} />
        </Stack.Title>
        <Stack.Toolbar placement="left">
          <Stack.Toolbar.Button
            icon="xmark"
            accessibilityLabel={t('chat.closeSearch')}
            onPress={search.onClose}
          />
        </Stack.Toolbar>
        <Stack.Toolbar placement="right">
          <Stack.Toolbar.Button
            icon="chevron.up"
            accessibilityLabel={t('chat.olderSearchResult')}
            disabled={
              search.state.loading ||
              search.state.index < 0 ||
              (!search.state.cursor && search.state.index >= search.state.hits.length - 1)
            }
            onPress={() => search.onMove('older')}
          />
          <Stack.Toolbar.Button
            icon="chevron.down"
            accessibilityLabel={t('chat.newerSearchResult')}
            disabled={search.state.loading || search.state.index <= 0}
            onPress={() => search.onMove('newer')}
          />
        </Stack.Toolbar>
      </>
    );
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
                  { icon: 'magnifyingglass', label: t('chat.searchThisChat'), unavailable: false },
                ] as const
              ).map(({ icon, label, unavailable }) => (
                <Button
                  key={icon}
                  systemImage={icon}
                  label={label}
                  onPress={unavailable ? undefined : search?.onOpen}
                  modifiers={[
                    buttonStyle('plain'),
                    labelStyle('iconOnly'),
                    font({ size: 19 }),
                    foregroundStyle(colors.text),
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
