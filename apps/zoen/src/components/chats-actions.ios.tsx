import { Host } from '@expo/ui';
import { Button, HStack } from '@expo/ui/swift-ui';
import { buttonStyle, font, foregroundStyle, frame, labelStyle } from '@expo/ui/swift-ui/modifiers';
import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import { router } from 'expo-router';
import { View } from 'react-native';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';

export function ChatsActions() {
  const { colors, scheme } = useTheme();
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
            foregroundStyle(colors.text),
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
            foregroundStyle(colors.text),
            frame({ width: 48, height: 44 }),
          ]}
        />
      </HStack>
    </Host>
  );
  return isLiquidGlassAvailable() ? (
    <GlassView
      isInteractive
      colorScheme={scheme}
      glassEffectStyle="regular"
      style={{ borderRadius: 24 }}
    >
      {content}
    </GlassView>
  ) : (
    <View style={{ borderRadius: 24, backgroundColor: colors.surface }}>{content}</View>
  );
}
