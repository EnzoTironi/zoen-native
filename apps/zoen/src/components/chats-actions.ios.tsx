import { Host } from '@expo/ui';
import { Button, HStack } from '@expo/ui/swift-ui';
import { buttonStyle, font, foregroundStyle, frame, labelStyle } from '@expo/ui/swift-ui/modifiers';
import { router } from 'expo-router';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { NavigationGlass } from './navigation-glass';

export function ChatsActions() {
  const { presentation: p, scheme } = useTheme();
  const content = (
    <Host style={{ width: 96, height: 44 }} colorScheme={scheme}>
      <HStack spacing={0}>
        <Button
          label={t('chat.searchAllHistory')}
          systemImage="magnifyingglass"
          onPress={() => router.push('/search')}
          modifiers={[
            buttonStyle('plain'),
            labelStyle('iconOnly'),
            font({ size: 19 }),
            foregroundStyle(p.navigation.control),
            frame({ width: 48, height: 44 }),
          ]}
        />
        <Button
          label={t('chat.newChat')}
          systemImage="square.and.pencil"
          onPress={() => router.push('/new-chat')}
          modifiers={[
            buttonStyle('plain'),
            labelStyle('iconOnly'),
            font({ size: 19 }),
            foregroundStyle(p.navigation.control),
            frame({ width: 48, height: 44 }),
          ]}
        />
      </HStack>
    </Host>
  );
  return <NavigationGlass>{content}</NavigationGlass>;
}
