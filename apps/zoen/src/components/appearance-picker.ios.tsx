import { useState } from 'react';
import { View } from 'react-native';
import { BottomSheet, Button, Column, Host, List, ListItem, Text } from '@expo/ui';
import { accessibilityAddTraits, accessibilityLabel, frame } from '@expo/ui/swift-ui/modifiers';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { resolveSkyTheme, SKY_PHASES, type SkyPhase, type ThemePreference } from '../theme/sky.ts';
import { Icon } from './primitives';
import { currentSkyLabel, skyLabels } from './appearance-options';
import { SkyLayer } from './sky-background';

const previews = Object.fromEntries(
  SKY_PHASES.map((phase) => [phase, resolveSkyTheme(phase, new Date(2020, 0, 1)).sky]),
) as Record<SkyPhase, ReturnType<typeof resolveSkyTheme>['sky']>;
function Swatch({ phase }: { phase: SkyPhase }) {
  const sky = previews[phase];
  return (
    <View
      accessible={false}
      accessibilityElementsHidden
      style={{
        width: 68,
        height: 46,
        borderRadius: 12,
        borderCurve: 'continuous',
        overflow: 'hidden',
        backgroundColor: sky.middle,
        experimental_backgroundImage: `linear-gradient(to bottom, ${sky.top}, ${sky.middle} 55%, ${sky.bottom})`,
      }}
    >
      <SkyLayer sky={sky} />
    </View>
  );
}
export function AppearancePicker() {
  const { preference, setPreference, colors, scheme, phase } = useTheme();
  const [open, setOpen] = useState(false);
  const labels = skyLabels();
  const current = currentSkyLabel(preference, phase);
  const [automaticPreview, setAutomaticPreview] = useState<SkyPhase>(phase);
  const automaticPhase = preference === 'auto' ? phase : automaticPreview;
  function select(value: ThemePreference) {
    setPreference(value);
    setOpen(false);
  }
  return (
    <Host
      matchContents={{ vertical: true }}
      style={{ width: '100%' }}
      colorScheme={scheme}
      seedColor={colors.accent}
    >
      <ListItem
        testID="appearance-picker"
        onPress={() => {
          setAutomaticPreview(resolveSkyTheme('auto', new Date()).phase);
          setOpen(true);
        }}
        leading={<Swatch phase={phase} />}
        trailing={
          <Icon
            name={{ ios: 'chevron.right', android: 'chevron_right' }}
            size={15}
            color={colors.secondary}
          />
        }
        supportingText={
          preference === 'auto' ? t('settings.skyFollowsLocalTime') : t('settings.skyChooseMoment')
        }
        modifiers={[
          frame({ minHeight: 64 }),
          accessibilityLabel(`${t('settings.appearance')}, ${current}`),
        ]}
      >
        {current}
      </ListItem>
      <BottomSheet
        testID="sky-picker-sheet"
        isPresented={open}
        onDismiss={() => setOpen(false)}
        snapPoints={['full']}
        containerColor={colors.background}
      >
        <Column
          spacing={8}
          modifiers={[frame({ maxWidth: Infinity, maxHeight: Infinity, alignment: 'topLeading' })]}
        >
          <Text textStyle={{ color: colors.text, fontSize: 24, fontWeight: '700' }}>
            {t('settings.appearance')}
          </Text>
          <Text textStyle={{ color: colors.secondary, fontSize: 16 }}>
            {t('settings.skyChooseMoment')}
          </Text>
          <List testID="sky-choices">
            {(['auto', ...SKY_PHASES] as const).map((value) => (
              <ListItem
                key={value}
                testID={`sky-choice-${value}`}
                onPress={() => select(value)}
                leading={<Swatch phase={value === 'auto' ? automaticPhase : value} />}
                trailing={
                  preference === value ? (
                    <Icon name={{ ios: 'checkmark', android: 'check' }} color={colors.accent} />
                  ) : undefined
                }
                supportingText={value === 'auto' ? t('settings.skyFollowsLocalTime') : undefined}
                modifiers={[
                  frame({ minHeight: 64 }),
                  accessibilityLabel(
                    value === 'auto' ? currentSkyLabel('auto', automaticPhase) : labels[value],
                  ),
                  ...(preference === value ? [accessibilityAddTraits(['isSelected'])] : []),
                ]}
              >
                {labels[value]}
              </ListItem>
            ))}
          </List>
          <Button
            label={t('common.oK')}
            variant="text"
            onPress={() => setOpen(false)}
            modifiers={[frame({ minHeight: 44 })]}
          />
        </Column>
      </BottomSheet>
    </Host>
  );
}
