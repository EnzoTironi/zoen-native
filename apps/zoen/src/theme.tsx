import { t } from './i18n/index.ts';
import { createContext, use, useEffect, useRef, useState, type ReactNode } from 'react';
import { useColorScheme } from 'react-native';
import { preferences } from './preferences';

const palettes = {
  light: {
    background: '#F4F6FB',
    surface: '#FFFFFF',
    raised: '#FFFFFF',
    muted: '#ECEFF6',
    line: '#E1E5EE',
    text: '#0E1320',
    secondary: '#5B6478',
    tertiary: '#8C95A8',
    accent: '#3D7A28',
    onAccent: '#FFFFFF',
    mine: '#111113',
    onMine: '#FFFFFF',
    theirs: '#F2F2F2',
    danger: '#CB3046',
    read: '#1F8FD8',
  },
  dark: {
    background: '#090D15',
    surface: '#121826',
    raised: '#182032',
    muted: '#1A2233',
    line: '#253047',
    text: '#EEF2FA',
    secondary: '#93A0B8',
    tertiary: '#5F6B82',
    accent: '#86C25A',
    onAccent: '#0E1320',
    mine: '#F2F2F0',
    onMine: '#111113',
    theirs: '#26272B',
    danger: '#FF6575',
    read: '#53BDEB',
  },
};
type Preference = 'system' | 'light' | 'dark';
const Context = createContext<{
  preference: Preference;
  setPreference: (value: Preference) => void;
  preferenceError: string | null;
} | null>(null);
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, updatePreference] = useState<Preference>('system');
  const [preferenceError, setPreferenceError] = useState<string | null>(null);
  const revision = useRef(0);
  const writes = useRef<Promise<void>>(Promise.resolve());
  useEffect(() => {
    let active = true;
    void preferences
      .get('appearance')
      .then((value) => {
        if (
          active &&
          revision.current === 0 &&
          (value === 'light' || value === 'dark' || value === 'system')
        )
          updatePreference(value);
      })
      .catch(() => {
        if (active) setPreferenceError(t('settings.yourAppearancePreferenceCouldNotBeRestored'));
      });
    return () => {
      active = false;
    };
  }, []);
  function setPreference(value: Preference) {
    const version = ++revision.current;
    updatePreference(value);
    writes.current = writes.current
      .catch(() => {})
      .then(() => preferences.set('appearance', value));
    void writes.current
      .then(() => {
        if (version === revision.current) setPreferenceError(null);
      })
      .catch(() => {
        if (version === revision.current)
          setPreferenceError(t('settings.appearanceChangedButThePreferenceCouldNotBe'));
      });
  }
  return <Context value={{ preference, setPreference, preferenceError }}>{children}</Context>;
}
export function useTheme() {
  const context = use(Context);
  const system = useColorScheme();
  if (!context) throw new Error('ThemeProvider is missing.');
  const scheme =
    context.preference === 'system' ? (system === 'dark' ? 'dark' : 'light') : context.preference;
  return { ...context, scheme, colors: palettes[scheme] };
}
