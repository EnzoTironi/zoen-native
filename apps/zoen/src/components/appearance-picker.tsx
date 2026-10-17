import { t } from '../i18n';
import { View } from 'react-native';
import { useTheme } from '../theme';
import { NativeButton } from './primitives';

export function AppearancePicker() {
  const { preference, setPreference } = useTheme();
  return (
    <View style={{ flexDirection: 'row', gap: 10, flexWrap: 'wrap' }}>
      {(
        [
          { value: 'light', label: t('settings.light') },
          { value: 'system', label: t('settings.system') },
          { value: 'dark', label: t('settings.dark') },
        ] as const
      ).map((item) => (
        <NativeButton
          key={item.value}
          label={item.label}
          secondary={preference !== item.value}
          onPress={() => setPreference(item.value)}
        />
      ))}
    </View>
  );
}
