import { t } from './i18n/index.ts';
import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { AccessibilityInfo, Appearance, AppState, Platform } from 'react-native';
import { preferences } from './preferences';
import {
  sampleSkyClock,
  normalizeAppearance,
  resolveSkyTheme,
  type ThemePreference,
} from './theme/sky.ts';

export type { SkyPhase, SkyColors, SemanticColors, ThemePreference } from './theme/sky.ts';
const CLOCK_INTERVAL = 30_000;
type Theme = ReturnType<typeof resolveSkyTheme> & {
  preference: ThemePreference;
  setPreference: (value: ThemePreference) => void;
  preferenceError: string | null;
  reducedMotion: boolean;
};
const Context = createContext<Theme | null>(null);
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, updatePreference] = useState<ThemePreference>('auto');
  const [clock, setClock] = useState(() => sampleSkyClock(new Date()));
  const [reducedMotion, setReducedMotion] = useState(false);
  const [preferenceError, setPreferenceError] = useState<string | null>(null);
  const revision = useRef(0);
  const writes = useRef<Promise<void>>(Promise.resolve());
  useEffect(() => {
    let active = true;
    let changed = false;
    const subscription = AccessibilityInfo.addEventListener('reduceMotionChanged', (value) => {
      changed = true;
      setReducedMotion(value);
    });
    void AccessibilityInfo.isReduceMotionEnabled()
      .then((value) => {
        if (active && !changed) setReducedMotion(value);
      })
      .catch(() => {});
    return () => {
      active = false;
      subscription.remove();
    };
  }, []);
  useEffect(() => {
    if (preference !== 'auto') return;
    let interval: ReturnType<typeof setInterval> | undefined;
    const stop = () => {
      if (interval !== undefined) clearInterval(interval);
      interval = undefined;
    };
    const refresh = () => {
      stop();
      // Date's local getters pick up timezone, daylight-saving and clock changes.
      setClock(sampleSkyClock(new Date()));
      interval = setInterval(
        () => setClock((previous) => sampleSkyClock(new Date(), previous)),
        CLOCK_INTERVAL,
      );
    };
    if (AppState.currentState === 'active') refresh();
    const subscription = AppState.addEventListener('change', (state) => {
      if (state === 'active') refresh();
      else stop();
    });
    return () => {
      stop();
      subscription.remove();
    };
  }, [preference]);
  const resolved = useMemo(
    () => (preference === 'auto' ? clock.automatic : resolveSkyTheme(preference, clock.date)),
    [preference, clock],
  );
  useEffect(() => {
    if (Platform.OS === 'ios') Appearance.setColorScheme(resolved.scheme);
  }, [resolved.scheme]);
  useEffect(() => {
    let active = true;
    void preferences
      .get('appearance')
      .then((value) => {
        if (active && revision.current === 0) updatePreference(normalizeAppearance(value));
      })
      .catch(() => {
        if (active && revision.current === 0)
          setPreferenceError(t('settings.yourAppearancePreferenceCouldNotBeRestored'));
      });
    return () => {
      active = false;
    };
  }, []);
  const setPreference = useCallback((value: ThemePreference) => {
    const version = ++revision.current;
    // Refresh immediately even if Auto was already selected before backgrounding.
    setClock(sampleSkyClock(new Date()));
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
  }, []);
  const value = useMemo<Theme>(
    () => ({
      ...resolved,
      sky: {
        ...resolved.sky,
        transitionMs: !reducedMotion && preference === 'auto' && clock.timed ? CLOCK_INTERVAL : 0,
      },
      preference,
      setPreference,
      preferenceError,
      reducedMotion,
    }),
    [
      resolved,
      reducedMotion,

      preference,
      clock.timed,
      setPreference,
      preferenceError,
    ],
  );
  return <Context value={value}>{children}</Context>;
}
export function useTheme() {
  const context = use(Context);
  if (!context) throw new Error('ThemeProvider is missing.');
  return context;
}
