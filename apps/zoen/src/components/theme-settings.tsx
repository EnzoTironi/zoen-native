import { Host, Slider, Switch } from '@expo/ui';
import {
  accessibilityLabel,
  accessibilityValue,
  foregroundStyle,
  frame,
} from '@expo/ui/swift-ui/modifiers';
import { useState, type ReactNode } from 'react';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { resolveSkyTheme } from '../theme/sky';
import {
  DEFAULT_THEME_CUSTOMIZATION,
  telegramPresentation,
  type ThemeCustomization,
  type ThemeProfile,
} from '../theme/telegram';
import { SettingsScreen } from './settings-screen';
import { SkyLayer } from './sky-background';
import { ThemeColorPicker } from './theme-color-picker';
import { THEME_PROFILES, isDarkTheme } from '../theme/catalog';

const accentColors = [
  '#0088FF',
  '#D33213',
  '#F08200',
  '#EDB400',
  '#70BB23',
  '#5396FA',
  '#9472EE',
  '#EB6CA4',
];

// Ported geometry: ThemeSettingsThemeIconItemNode (116pt row, 98×62 image).
function Preview({
  profile,
  selected,
  onPress,
}: {
  profile: ThemeProfile;
  selected: boolean;
  onPress(): void;
}) {
  const { customizations, presentation: p } = useTheme();
  const resolved = resolveSkyTheme(profile, new Date(2026, 0, 1), customizations[profile]);
  const preview = resolved.presentation;
  const label = themeLabels()[profile];
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected }}
      accessibilityLabel={label}
      onPress={onPress}
      testID={`theme-preset-${profile}`}
      style={({ pressed }) => ({ width: 116, height: 116, opacity: pressed ? 0.65 : 1 })}
    >
      <View
        style={{
          position: 'absolute',
          left: 9,
          top: 13,
          width: 100,
          height: 64,
          borderRadius: 17,
          padding: 1,
          borderWidth: selected ? 2 : 1,
          borderColor: selected ? p.accent : p.list.disclosure,
        }}
      >
        <View style={{ flex: 1, borderRadius: 14, overflow: 'hidden' }}>
          <SkyLayer sky={resolved.sky} />
          <View style={{ height: 10, backgroundColor: preview.navigation.background }} />
          <View style={{ margin: 6, gap: 5 }}>
            <View
              style={{
                width: 48,
                height: 13,
                borderRadius: 7,
                backgroundColor: preview.chat.incoming.fill,
              }}
            />
            <View
              style={{
                width: 56,
                height: 13,
                alignSelf: 'flex-end',
                borderRadius: 7,
                backgroundColor: preview.chat.outgoing.fill,
                experimental_backgroundImage:
                  preview.chat.outgoing.fillColors.length > 1
                    ? `linear-gradient(to bottom, ${preview.chat.outgoing.fillColors.join(', ')})`
                    : undefined,
              }}
            />
          </View>
        </View>
      </View>
      <Text
        numberOfLines={1}
        style={{
          position: 'absolute',
          top: 88,
          width: '100%',
          textAlign: 'center',
          fontSize: 12,
          fontWeight: selected ? '700' : '400',
          color: selected ? p.accent : p.text,
        }}
      >
        {label}
      </Text>
    </Pressable>
  );
}
function themeLabels(): Record<ThemeProfile, string> {
  return {
    day: t('settings.skyDay'),
    night: t('settings.skyNight'),
    'telegram-classic': t('settings.themeClassic'),
    'telegram-day': t('settings.themeDay'),
    'telegram-night': t('settings.themeNight'),
    'telegram-tinted': t('settings.themeTinted'),
  };
}

function Block({
  title,
  children,
  footer,
}: {
  title?: string;
  children: ReactNode;
  footer?: string;
}) {
  const { presentation: p } = useTheme();
  return (
    <View style={{ gap: 7 }}>
      {title ? (
        <Text
          accessibilityRole="header"
          style={{
            fontSize: 13,
            color: p.list.section,
            marginLeft: 16,
            textTransform: 'uppercase',
          }}
        >
          {title}
        </Text>
      ) : null}
      <View style={{ backgroundColor: p.list.block, borderRadius: 10, overflow: 'hidden' }}>
        {children}
      </View>
      {footer ? (
        <Text style={{ fontSize: 13, lineHeight: 18, color: p.list.section, marginHorizontal: 16 }}>
          {footer}
        </Text>
      ) : null}
    </View>
  );
}
function Toggle({
  label,
  value,
  onChange,
}: {
  label: string;
  value: boolean;
  onChange(value: boolean): void;
}) {
  const { presentation: p } = useTheme();
  return (
    <View
      style={{
        minHeight: 50,
        paddingHorizontal: 16,
        justifyContent: 'center',
      }}
    >
      <Host matchContents style={{ width: '100%' }}>
        <Switch
          label={label}
          value={value}
          onValueChange={onChange}
          modifiers={
            process.env.EXPO_OS === 'ios'
              ? [frame({ minHeight: 50, maxWidth: Infinity }), foregroundStyle(p.list.text)]
              : undefined
          }
        />
      </Host>
    </View>
  );
}
function ColorRow({
  label,
  value,
  colors,
  onChange,
}: {
  label: string;
  value: string;
  colors: string[];
  onChange(value: string): void;
}) {
  const { presentation: p } = useTheme();
  return (
    <View style={{ paddingHorizontal: 16, paddingTop: 12, paddingBottom: 8, gap: 6 }}>
      <Text style={{ color: p.list.text, fontSize: 17 }}>{label}</Text>
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        contentContainerStyle={{ alignItems: 'center', gap: 4 }}
      >
        {colors.map((color) => (
          <Pressable
            key={color}
            onPress={() => onChange(color)}
            accessibilityRole="button"
            accessibilityLabel={`${label}, ${color}`}
            accessibilityState={{ selected: value.toUpperCase() === color.toUpperCase() }}
            style={{
              width: 44,
              height: 44,
              borderRadius: 22,
              padding: 5,
              borderWidth: value.toUpperCase() === color.toUpperCase() ? 2 : 0,
              borderColor: p.accent,
            }}
          >
            <View
              style={{
                flex: 1,
                borderRadius: 20,
                backgroundColor: color,
                borderWidth: 0.5,
                borderColor: p.separator,
              }}
            />
          </Pressable>
        ))}
        <ThemeColorPicker label={label} value={value} onChange={onChange} />
      </ScrollView>
    </View>
  );
}

/** Every preset edits the same presentation owner used by the running app. */
export function ThemeSettings() {
  const {
    presentation: p,
    preference,
    profile,
    setPreference,
    customizations,
    setCustomization,
    preferenceError,
  } = useTheme();
  const [editing, setEditing] = useState<ThemeProfile>(profile);
  const settings = customizations[editing];
  const preview = telegramPresentation(editing, settings);
  const sky = resolveSkyTheme(editing, new Date(2026, 0, 1), settings).sky;
  const update = (patch: Partial<ThemeCustomization>) => setCustomization(editing, patch);
  function select(profile: ThemeProfile) {
    setEditing(profile);
    setPreference(profile);
  }
  return (
    <SettingsScreen title={t('settings.appearance')} testID="theme-settings-screen">
      <Block title={t('settings.themeChoose')}>
        <ScrollView
          horizontal
          showsHorizontalScrollIndicator={false}
          contentContainerStyle={{ paddingHorizontal: 6 }}
        >
          {THEME_PROFILES.map((theme) => (
            <Preview
              key={theme}
              profile={theme}
              selected={editing === theme}
              onPress={() => select(theme)}
            />
          ))}
        </ScrollView>
      </Block>
      <View style={{ minHeight: 176, borderRadius: 10, overflow: 'hidden', padding: 12, gap: 10 }}>
        <SkyLayer sky={sky} />
        <View
          style={{
            alignSelf: 'flex-start',
            maxWidth: '84%',
            padding: 11,
            borderRadius: 16,
            backgroundColor: preview.chat.incoming.fill,
          }}
        >
          <Text style={{ color: preview.chat.incoming.text, fontSize: settings.textSize }}>
            {t('settings.themePreviewReceived')}
          </Text>
        </View>
        <View
          style={{
            alignSelf: 'flex-end',
            maxWidth: '84%',
            padding: 11,
            borderRadius: 16,
            backgroundColor: preview.chat.outgoing.fill,
            experimental_backgroundImage:
              preview.chat.outgoing.fillColors.length > 1
                ? `linear-gradient(to bottom, ${preview.chat.outgoing.fillColors.join(', ')})`
                : undefined,
          }}
        >
          <Text style={{ color: preview.chat.outgoing.text, fontSize: settings.textSize }}>
            {t('settings.themePreviewOwn')}
          </Text>
        </View>
      </View>
      <Block footer={t('settings.skyFollowsLocalTime')}>
        <Toggle
          label={t('settings.themeAutomatic')}
          value={preference === 'auto'}
          onChange={(enabled) => setPreference(enabled ? 'auto' : editing)}
        />
      </Block>
      <Block title={t('settings.themeColors')}>
        <ColorRow
          label={t('settings.themeAccent')}
          value={preview.selectedAccent}
          colors={accentColors}
          onChange={(accent) => update({ accent })}
        />
        <ColorRow
          label={t('settings.themeMessages')}
          value={preview.chat.outgoing.fill}
          colors={
            !isDarkTheme(editing)
              ? ['#FFFFFF', '#E1FFC7', '#D9F4FF', '#FDE3CF', '#E4DAFC', '#F9D8E5']
              : ['#000000', '#313131', '#0088FF', '#194D48', '#493C58', '#5A3439']
          }
          onChange={(outgoing) => update({ outgoing })}
        />
      </Block>
      <Block title={t('settings.themeBackground')}>
        <ColorRow
          label={t('settings.themeSkyTop')}
          value={settings.backgroundTop ?? sky.top}
          colors={
            !isDarkTheme(editing)
              ? ['#ECF4FA', '#E3F2FF', '#F2F6F8', '#E8F5F3']
              : ['#080F19', '#07121F', '#0B1722', '#121826']
          }
          onChange={(backgroundTop) => update({ backgroundTop })}
        />
        <ColorRow
          label={t('settings.themeSkyBottom')}
          value={settings.backgroundBottom ?? sky.bottom}
          colors={
            !isDarkTheme(editing)
              ? ['#FBFCFC', '#ECF4FA', '#E4F0F8', '#F8FAFB']
              : ['#14212C', '#0D1723', '#172A37', '#101C29']
          }
          onChange={(backgroundBottom) => update({ backgroundBottom })}
        />
      </Block>
      <Block title={t('settings.themeTextSize')}>
        <View style={{ paddingHorizontal: 16, paddingVertical: 10 }}>
          <Text style={{ color: p.list.text, fontSize: 17 }}>
            {t('settings.themeTextSizeValue', { size: settings.textSize })}
          </Text>
          <Host style={{ height: 44, width: '100%' }}>
            <Slider
              value={settings.textSize}
              min={13}
              max={26}
              step={1}
              onValueChange={(textSize) => update({ textSize })}
              testID="theme-text-size"
              modifiers={
                process.env.EXPO_OS === 'ios'
                  ? [
                      accessibilityLabel(t('settings.themeTextSize')),
                      accessibilityValue(
                        t('settings.themeTextSizeValue', { size: settings.textSize }),
                      ),
                    ]
                  : undefined
              }
            />
          </Host>
          <Text
            style={{
              color: preview.chat.incoming.text,
              backgroundColor: preview.chat.incoming.fill,
              fontSize: settings.textSize,
              padding: 12,
              borderRadius: 12,
            }}
          >
            {t('settings.themePreviewReceived')}
          </Text>
        </View>
      </Block>
      <Block>
        <Toggle
          label={t('settings.themeLargeEmoji')}
          value={settings.largeEmoji}
          onChange={(largeEmoji) => update({ largeEmoji })}
        />
        <Toggle
          label={t('settings.themeReduceMotion')}
          value={settings.reduceMotion}
          onChange={(reduceMotion) => update({ reduceMotion })}
        />
      </Block>
      <Block>
        <Pressable
          onPress={() => setCustomization(editing, DEFAULT_THEME_CUSTOMIZATION)}
          accessibilityRole="button"
          style={({ pressed }) => ({
            minHeight: 50,
            paddingHorizontal: 16,
            justifyContent: 'center',
            opacity: pressed ? 0.6 : 1,
          })}
        >
          <Text style={{ color: p.list.accent, fontSize: 17 }}>{t('settings.themeReset')}</Text>
        </Pressable>
      </Block>
      {preferenceError ? (
        <Text accessibilityRole="alert" style={{ color: p.destructive }}>
          {preferenceError}
        </Text>
      ) : null}
    </SettingsScreen>
  );
}
