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
  DEFAULT_THEME_CUSTOMIZATION,
  decodeThemeCustomization,
  normalizeThemeCustomization,
  type TelegramPresentation,
  type ThemeCustomization,
  type ThemeProfile,
} from './theme/telegram';
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
  presentation: TelegramPresentation;
  localDate: Date;
  appearanceReady: boolean;
  customizations: Readonly<Record<ThemeProfile, ThemeCustomization>>;
  setCustomization: (profile: ThemeProfile, patch: Partial<ThemeCustomization>) => void;
};
const Context = createContext<Theme | null>(null);
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, updatePreference] = useState<ThemePreference>('auto');
  const [clock, setClock] = useState(() => sampleSkyClock(new Date()));
  const [reducedMotion, setReducedMotion] = useState(false);
  const [appearanceRestored, setAppearanceRestored] = useState(false);
  const [customizationsRestored, setCustomizationsRestored] = useState(false);
  const [preferenceErrors, setPreferenceErrors] = useState<
    Record<'appearance' | ThemeProfile, string | null>
  >({ appearance: null, day: null, night: null });
  const preferenceError = Object.values(preferenceErrors).find((error) => error !== null) ?? null;
  const setPreferenceError = useCallback(
    (key: 'appearance' | ThemeProfile, error: string | null) => {
      setPreferenceErrors((previous) => ({ ...previous, [key]: error }));
    },
    [],
  );
  const revision = useRef(0);
  const writes = useRef<Promise<void>>(Promise.resolve());
  const customizationRevision = useRef({ day: 0, night: 0 });
  const [customizations, setCustomizations] = useState<Record<ThemeProfile, ThemeCustomization>>({
    day: DEFAULT_THEME_CUSTOMIZATION,
    night: DEFAULT_THEME_CUSTOMIZATION,
  });
  // Latest admitted presentation values also cover native callbacks queued before React commits.
  // This is the merge snapshot for this provider's existing state/persistence owner.
  const customizationValues = useRef<Record<ThemeProfile, ThemeCustomization>>({
    day: DEFAULT_THEME_CUSTOMIZATION,
    night: DEFAULT_THEME_CUSTOMIZATION,
  });
  useEffect(() => {
    let current = true;
    let remaining = 2;
    for (const profile of ['day', 'night'] as const) {
      void preferences
        .get(`theme-customization-${profile}`)
        .then((value) => {
          if (current && customizationRevision.current[profile] === 0) {
            const next = {
              ...customizationValues.current,
              [profile]: decodeThemeCustomization(value),
            };
            customizationValues.current = next;
            setCustomizations(next);
          }
        })
        .catch(() => {
          if (current && customizationRevision.current[profile] === 0)
            setPreferenceError(profile, t('settings.yourAppearancePreferenceCouldNotBeRestored'));
        })
        .finally(() => {
          remaining--;
          if (current && remaining === 0) setCustomizationsRestored(true);
        });
    }
    return () => {
      current = false;
    };
  }, [setPreferenceError]);
  const setCustomization = useCallback(
    (profile: ThemeProfile, patch: Partial<ThemeCustomization>) => {
      const normalized = normalizeThemeCustomization({
        ...customizationValues.current[profile],
        ...patch,
      });
      const version = ++customizationRevision.current[profile];
      setClock(sampleSkyClock(new Date()));
      const next = { ...customizationValues.current, [profile]: normalized };
      customizationValues.current = next;
      setCustomizations(next);
      writes.current = writes.current
        .catch(() => {})
        .then(async () => {
          if (version === customizationRevision.current[profile])
            await preferences.set(`theme-customization-${profile}`, JSON.stringify(normalized));
        });
      void writes.current
        .then(() => {
          if (version === customizationRevision.current[profile]) setPreferenceError(profile, null);
        })
        .catch(() => {
          if (version === customizationRevision.current[profile])
            setPreferenceError(profile, t('settings.appearanceChangedButThePreferenceCouldNotBe'));
        });
    },
    [setPreferenceError],
  );
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
        () =>
          setClock((previous) => {
            const next = sampleSkyClock(new Date(), previous);
            // Two static sky families do not need an app-wide render every tick.
            // Still observe civil-day/timezone changes while the app stays open.
            return previous.automatic.phase === next.automatic.phase &&
              previous.timezoneOffset === next.timezoneOffset &&
              previous.date.toDateString() === next.date.toDateString()
              ? previous
              : next;
          }),
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
  }, []);
  const base = useMemo(
    () => (preference === 'auto' ? clock.automatic : resolveSkyTheme(preference, clock.date)),
    [preference, clock],
  );
  const profile = base.scheme === 'dark' ? 'night' : 'day';
  const selectedCustomization = customizations[profile];
  const resolved = useMemo(
    () => resolveSkyTheme(base.phase, clock.date, selectedCustomization),
    [base.phase, clock.date, selectedCustomization],
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
          setPreferenceError(
            'appearance',
            t('settings.yourAppearancePreferenceCouldNotBeRestored'),
          );
      })
      .finally(() => {
        if (active) setAppearanceRestored(true);
      });
    return () => {
      active = false;
    };
  }, [setPreferenceError]);
  const setPreference = useCallback(
    (value: ThemePreference) => {
      const version = ++revision.current;
      // Refresh immediately even if Auto was already selected before backgrounding.
      setClock(sampleSkyClock(new Date()));
      updatePreference(value);
      writes.current = writes.current
        .catch(() => {})
        .then(() => preferences.set('appearance', value));
      void writes.current
        .then(() => {
          if (version === revision.current) setPreferenceError('appearance', null);
        })
        .catch(() => {
          if (version === revision.current)
            setPreferenceError(
              'appearance',
              t('settings.appearanceChangedButThePreferenceCouldNotBe'),
            );
        });
    },
    [setPreferenceError],
  );
  const value = useMemo<Theme>(
    () => ({
      ...resolved,
      localDate: clock.date,
      appearanceReady: appearanceRestored && customizationsRestored,
      sky: {
        ...resolved.sky,
        transitionMs:
          !reducedMotion &&
          !selectedCustomization.reduceMotion &&
          preference === 'auto' &&
          clock.timed
            ? CLOCK_INTERVAL
            : 0,
      },
      preference,
      setPreference,
      preferenceError,
      reducedMotion: reducedMotion || selectedCustomization.reduceMotion,
      customizations,
      setCustomization,
    }),
    [
      resolved,
      reducedMotion,

      preference,
      clock.timed,
      clock.date,
      appearanceRestored,
      customizationsRestored,
      setPreference,
      preferenceError,
      customizations,
      setCustomization,
      selectedCustomization.reduceMotion,
    ],
  );
  return <Context value={value}>{children}</Context>;
}
export function useTheme() {
  const context = use(Context);
  if (!context) throw new Error('ThemeProvider is missing.');
  return context;
}
