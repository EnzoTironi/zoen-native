import { Host, ListItem } from '@expo/ui';
import { accessibilityLabel, frame } from '@expo/ui/swift-ui/modifiers';
import { router } from 'expo-router';
import { View } from 'react-native';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { currentSkyLabel } from './appearance-options';
import { Icon } from './primitives';
import { SkyLayer } from './sky-background';

export function AppearancePicker() {
  const { preference, presentation: p, scheme, phase, sky } = useTheme();
  return (
    <Host
      matchContents={{ vertical: true }}
      style={{ width: '100%' }}
      colorScheme={scheme}
      seedColor={p.accent}
    >
      <ListItem
        testID="appearance-picker"
        onPress={() => router.push('/settings/appearance')}
        leading={
          <View
            accessible={false}
            accessibilityElementsHidden
            style={{ width: 44, height: 44, borderRadius: 10, overflow: 'hidden' }}
          >
            <SkyLayer sky={sky} />
          </View>
        }
        trailing={
          <Icon
            name={{ ios: 'chevron.right', android: 'chevron_right' }}
            size={13}
            color={p.list.disclosure}
          />
        }
        supportingText={currentSkyLabel(preference, phase)}
        modifiers={[
          frame({ minHeight: 60 }),
          accessibilityLabel(`${t('settings.appearance')}, ${currentSkyLabel(preference, phase)}`),
        ]}
      >
        {t('settings.appearance')}
      </ListItem>
    </Host>
  );
}
