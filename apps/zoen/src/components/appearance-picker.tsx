import { Host, Picker } from '@expo/ui';
import { useTheme } from '../theme';
import { SKY_PHASES, type ThemePreference } from '../theme/sky.ts';
import { skyLabels } from './appearance-options';

export function AppearancePicker() {
  const { preference, setPreference, scheme } = useTheme();
  const labels = skyLabels();
  return (
    <Host matchContents={{ vertical: true }} style={{ width: '100%' }} colorScheme={scheme}>
      <Picker<ThemePreference>
        testID="appearance-picker"
        selectedValue={preference}
        onValueChange={setPreference}
        appearance="menu"
      >
        {(['auto', ...SKY_PHASES] as const).map((value) => (
          <Picker.Item key={value} label={labels[value]} value={value} />
        ))}
      </Picker>
    </Host>
  );
}
