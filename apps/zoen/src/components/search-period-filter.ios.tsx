import { BottomSheet, Host, RNHostView } from '@expo/ui';
import DateTimePicker from '@expo/ui/community/datetime-picker';
import { useFocusEffect } from 'expo-router';
import { useCallback, useEffect, useLayoutEffect, useState, useSyncExternalStore } from 'react';
import { AppState, ScrollView, Text, View } from 'react-native';
import { SearchDateRange, searchDateRange } from '../core/search-date-range';
import { t, formatLocale } from '../i18n';
import { useTheme } from '../theme';
import { NativeButton, IconButton } from './primitives';
import type { SearchPeriodFilterProps } from './search-period-filter.types';

export function SearchPeriodFilter(props: SearchPeriodFilterProps) {
  const { colors, scheme } = useTheme();
  const [range] = useState(() => new SearchDateRange());
  useLayoutEffect(
    () =>
      range.bind({
        current: () => AppState.currentState === 'active' && props.isCurrent(),
        apply: props.onChange,
      }),
    [range, props],
  );
  useFocusEffect(
    useCallback(() => {
      range.focus();
      return () => range.blur();
    }, [range]),
  );
  useEffect(() => {
    const app = AppState.addEventListener('change', (state) => {
      if (state !== 'active') range.close();
    });
    return () => app.remove();
  }, [range]);
  const state = useSyncExternalStore(range.subscribe, range.getSnapshot, range.getSnapshot);
  const valid = searchDateRange(new Date(state.start), new Date(state.end));
  return (
    <>
      <NativeButton
        testID="search-custom-period"
        secondary
        label={
          props.selected && props.value
            ? `${new Date(props.value.start_ms).toLocaleDateString(formatLocale)} – ${new Date(props.value.end_ms - 1).toLocaleDateString(formatLocale)}`
            : t('chat.searchCustomDates')
        }
        onPress={() => range.open(props.value)}
      />
      <Host
        style={{ position: 'absolute' }}
        pointerEvents="none"
        colorScheme={scheme}
        seedColor={colors.accent}
      >
        <BottomSheet
          key={state.token}
          testID="search-period-sheet"
          isPresented={state.presented}
          onDismiss={() => range.close(state.token)}
          snapPoints={['half', 'full']}
          containerColor={colors.surface}
        >
          <RNHostView>
            <ScrollView contentContainerStyle={{ padding: 20, paddingBottom: 32, gap: 12 }}>
              <View style={{ flexDirection: 'row', alignItems: 'center' }}>
                <Text
                  accessibilityRole="header"
                  style={{ flex: 1, color: colors.text, fontSize: 20, fontWeight: '600' }}
                >
                  {t('chat.searchCustomDates')}
                </Text>
                <IconButton
                  name={{ ios: 'xmark', android: 'close', web: 'close' }}
                  label={t('common.close')}
                  onPress={() => range.close()}
                />
              </View>
              {(['start', 'end'] as const).map((field) => (
                <View key={field}>
                  <Text style={{ color: colors.secondary }}>
                    {t(field === 'start' ? 'chat.searchFromDate' : 'chat.searchThroughDate')}
                  </Text>
                  <DateTimePicker
                    testID={`search-period-${field}`}
                    mode="date"
                    display="compact"
                    value={new Date(state[field])}
                    locale={formatLocale}
                    themeVariant={scheme}
                    onValueChange={(_event, value) => {
                      if (value) range.pick(state.token, field, value);
                    }}
                  />
                </View>
              ))}
              {!valid ? (
                <Text accessibilityRole="alert" style={{ color: colors.danger }}>
                  {t('chat.searchInvalidDates')}
                </Text>
              ) : null}
              <NativeButton
                testID="search-period-apply"
                fullWidth
                label={t('common.oK')}
                disabled={!valid}
                onPress={() => range.apply(state)}
              />
            </ScrollView>
          </RNHostView>
        </BottomSheet>
      </Host>
    </>
  );
}
