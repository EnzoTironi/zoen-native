import { t } from '../i18n';
import { Host } from '@expo/ui';
import { Picker, Text } from '@expo/ui/swift-ui';
import { frame, pickerStyle, tag } from '@expo/ui/swift-ui/modifiers';
import { useWindowDimensions } from 'react-native';
import { useTheme } from '../theme';

export function AppearancePicker() {
  const { colors, scheme, preference, setPreference } = useTheme();
  const { fontScale } = useWindowDimensions();
  return (
    <Host
      matchContents={{ vertical: true }}
      style={{ width: '100%' }}
      colorScheme={scheme}
      seedColor={colors.accent}
    >
      <Picker
        testID="appearance-picker"
        label={t('settings.appearance')}
        selection={preference}
        onSelectionChange={setPreference}
        modifiers={[pickerStyle(fontScale > 1.3 ? 'menu' : 'segmented'), frame({ minHeight: 44 })]}
      >
        <Text modifiers={[tag('light')]}>{t('settings.light')}</Text>
        <Text modifiers={[tag('system')]}>{t('settings.system')}</Text>
        <Text modifiers={[tag('dark')]}>{t('settings.dark')}</Text>
      </Picker>
    </Host>
  );
}
