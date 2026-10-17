import { t, locale, formatLocale } from '../i18n/index.ts';
import { Pressable, Text, View } from 'react-native';
import { NativeButton, Surface } from '../components/primitives';
import { formatMoney } from '../i18n/money.ts';
import type { AppStateDto } from '../core/generated/AppStateDto';
import { useTheme } from '../theme';
import { parseMiniappJson } from './json';

function object(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
function list(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.map(object) : [];
}
function text(value: unknown): string {
  return typeof value === 'string' ? value : '';
}
function count(value: unknown): number {
  return Array.isArray(value) ? value.length : 0;
}
function exactMoney(value: unknown, currency: unknown): string {
  const amount =
    typeof value === 'string' && /^-?\d+$/.test(value)
      ? value
      : typeof value === 'number' && Number.isSafeInteger(value)
        ? String(value)
        : null;
  if (amount === null || typeof currency !== 'string' || !/^[A-Z]{3}$/.test(currency))
    return t('miniapps.amountUnavailable');
  try {
    return formatMoney(amount, { locale, currency, minorUnitDigits: 2 });
  } catch {
    return t('miniapps.amountUnavailable');
  }
}
function time(value: unknown, zone?: unknown): string {
  const milliseconds = typeof value === 'string' && /^\d+$/.test(value) ? Number(value) : value;
  if (typeof milliseconds !== 'number' || !Number.isSafeInteger(milliseconds))
    return t('miniapps.dateUnavailable');
  try {
    return new Date(milliseconds).toLocaleString(
      formatLocale,
      typeof zone === 'string' ? { timeZone: zone } : undefined,
    );
  } catch {
    return t('miniapps.dateUnavailable');
  }
}
export function MiniappState({
  app,
  busy,
  onCall,
}: {
  app: AppStateDto;
  busy: boolean;
  onCall: (tool: string, args: Record<string, unknown>) => void;
}) {
  const { colors } = useTheme();
  let state: Record<string, unknown>;
  try {
    state = object(parseMiniappJson(app.view_json));
  } catch {
    return (
      <Text accessibilityRole="alert" style={{ color: colors.danger }}>
        {t('miniapps.thisMiniappSStateCouldNotBeOpened')}
      </Text>
    );
  }
  return (
    <View style={{ gap: 18 }}>
      <Text selectable style={{ color: colors.secondary, fontSize: 17, lineHeight: 25 }}>
        {app.headline}
      </Text>
      {app.metrics.length ? (
        <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 12 }}>
          {app.metrics.map((metric) => (
            <Surface key={metric.label} style={{ minWidth: 140, flexGrow: 1 }}>
              <Text style={{ color: colors.secondary, fontSize: 13 }}>{metric.label}</Text>
              <View
                accessibilityRole="progressbar"
                accessibilityLabel={metric.label}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={Math.round(Math.min(1, Math.max(0, metric.value)) * 100)}
                style={{
                  height: 6,
                  borderRadius: 3,
                  backgroundColor: colors.muted,
                  overflow: 'hidden',
                }}
              >
                <View
                  style={{
                    height: 6,
                    backgroundColor: colors.accent,
                    width: `${Math.min(1, Math.max(0, metric.value)) * 100}%`,
                  }}
                />
              </View>
            </Surface>
          ))}
        </View>
      ) : null}
      {app.app_id === 'pet' ? (
        <Surface>
          <Text accessibilityElementsHidden style={{ fontSize: 82, textAlign: 'center' }}>
            {state.released ? '🌿' : state.asleep ? '💤' : '🫏'}
          </Text>
          <Text
            style={{ color: colors.text, fontSize: 24, fontWeight: '600', textAlign: 'center' }}
          >
            {text(state.name)}
          </Text>
          <Text style={{ color: colors.secondary, textAlign: 'center' }}>{text(state.mood)}</Text>
          {!state.released ? (
            <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 10 }}>
              <NativeButton
                label={t('miniapps.feed')}
                disabled={busy || state.asleep === true}
                onPress={() => onCall('pet_feed', {})}
              />
              <NativeButton
                secondary
                label={state.asleep ? t('miniapps.wakeUp') : t('miniapps.nap')}
                disabled={busy}
                onPress={() => onCall(state.asleep ? 'pet_wake' : 'pet_nap', {})}
              />
              <NativeButton
                secondary
                label={t('miniapps.play')}
                disabled={busy || state.asleep === true}
                onPress={() => onCall('pet_play', {})}
              />
            </View>
          ) : null}
        </Surface>
      ) : null}
      {app.app_id === 'poll' ? (
        <Surface>
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 24, fontWeight: '600' }}
          >
            {text(state.question)}
          </Text>
          {list(state.options).map((option) => (
            <Pressable
              key={text(option.id)}
              disabled={busy || state.closed === true}
              accessibilityRole="button"
              accessibilityLabel={t('miniapps.voteForOption', {
                option: text(option.text),
                count: count(option.votes),
              })}
              onPress={() => onCall('poll_vote', { option: text(option.id) })}
              style={{
                minHeight: 56,
                padding: 16,
                gap: 8,
                borderRadius: 16,
                backgroundColor: colors.muted,
              }}
            >
              <Text style={{ color: colors.text, fontSize: 17 }}>{text(option.text)}</Text>
              <Text style={{ color: colors.secondary, fontSize: 12 }}>
                {t('miniapps.voteCount', { count: count(option.votes) })}
              </Text>
            </Pressable>
          ))}
          {state.closed ? (
            <Text style={{ color: colors.accent, fontWeight: '600' }}>
              {t('miniapps.pollDecided', { winner: text(state.winner) })}
            </Text>
          ) : (
            <NativeButton
              secondary
              label={t('miniapps.closePollAndAddToPlan')}
              disabled={busy}
              onPress={() => onCall('poll_close', {})}
            />
          )}
        </Surface>
      ) : null}
      {app.app_id === 'list' ? (
        <Surface>
          {list(state.items).map((item) => (
            <Pressable
              key={text(item.id)}
              disabled={busy}
              accessibilityRole="checkbox"
              aria-checked={item.done === true}
              accessibilityLabel={text(item.text)}
              onPress={() => onCall('list_toggle', { id: text(item.id) })}
              style={{
                minHeight: 52,
                flexDirection: 'row',
                alignItems: 'center',
                gap: 12,
                paddingVertical: 10,
              }}
            >
              <Text style={{ color: colors.accent, fontSize: 23 }}>{item.done ? '✓' : '○'}</Text>
              <View style={{ flex: 1, gap: 4 }}>
                <Text
                  selectable
                  style={{
                    color: item.done ? colors.secondary : colors.text,
                    fontSize: 17,
                    textDecorationLine: item.done ? 'line-through' : 'none',
                  }}
                >
                  {text(item.text)}
                </Text>
                {text(item.by) ? (
                  <Text style={{ color: colors.secondary, fontSize: 12 }}>{text(item.by)}</Text>
                ) : null}
              </View>
            </Pressable>
          ))}
        </Surface>
      ) : null}
      {app.app_id === 'recipe' ? (
        <>
          <Surface>
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 22, fontWeight: '600' }}
            >
              {text(state.title)}
            </Text>
            <Text style={{ color: colors.secondary }}>
              {t('miniapps.servingsCount', { total: String(state.servings ?? '—') })}
            </Text>
            {list(state.ingredients).map((ingredient) => (
              <Pressable
                key={text(ingredient.id)}
                accessibilityRole="checkbox"
                aria-checked={ingredient.done === true}
                disabled={busy}
                onPress={() => onCall('recipe_check', { id: text(ingredient.id) })}
                style={{ minHeight: 48, flexDirection: 'row', gap: 12, alignItems: 'center' }}
              >
                <Text style={{ color: colors.accent }}>{ingredient.done ? '✓' : '○'}</Text>
                <Text style={{ color: colors.text, flex: 1 }}>
                  {text(ingredient.name) || text(ingredient.text)}
                  {ingredient.qty === undefined
                    ? ''
                    : ` · ${typeof ingredient.qty === 'number' && typeof state.servings === 'number' && typeof state.baseServings === 'number' && state.baseServings > 0 ? ((ingredient.qty * state.servings) / state.baseServings).toLocaleString(formatLocale, { maximumFractionDigits: 2 }) : String(ingredient.qty)} ${text(ingredient.unit)}`}
                </Text>
              </Pressable>
            ))}
          </Surface>
          {Array.isArray(state.steps) ? (
            <Surface>
              <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>
                {t('miniapps.cookingMode')}
              </Text>
              {state.steps.map((step, index) => (
                <Pressable
                  key={index}
                  disabled={busy}
                  accessibilityRole="button"
                  onPress={() => onCall('recipe_cook', { step: index })}
                  style={{ minHeight: 48, gap: 6, paddingVertical: 10 }}
                >
                  <Text style={{ color: colors.accent, fontWeight: '600' }}>
                    {t('miniapps.stepNumber', { position: index + 1 })}
                  </Text>
                  <Text style={{ color: colors.text, lineHeight: 23 }}>
                    {typeof step === 'string' ? step : text(object(step).text)}
                  </Text>
                </Pressable>
              ))}
            </Surface>
          ) : null}
        </>
      ) : null}
      {app.app_id === 'countdown' ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 30, fontWeight: '600' }}>
            {text(state.title)}
          </Text>
          <Text style={{ color: colors.secondary, fontSize: 18 }}>{time(state.target)}</Text>
          {text(state.place) ? (
            <Text style={{ color: colors.secondary }}>{text(state.place)}</Text>
          ) : null}
        </Surface>
      ) : null}
      {app.app_id === 'hike' ? (
        <>
          <Surface>
            <Text style={{ color: colors.text, fontSize: 22, fontWeight: '600' }}>
              {text(state.title)}
            </Text>
            <Text style={{ color: colors.secondary }}>
              {text(state.day)} · {text(state.area)}
            </Text>
            {list(state.trails).map((trail) => (
              <Pressable
                key={text(trail.id)}
                disabled={busy || !!state.decided}
                accessibilityRole="button"
                onPress={() => onCall('hike_vote', { trail: text(trail.id) })}
                style={{ padding: 16, gap: 6, borderRadius: 16, backgroundColor: colors.muted }}
              >
                <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
                  {text(trail.name)}
                </Text>
                <Text style={{ color: colors.secondary }}>
                  {t('miniapps.voteCount', { count: count(trail.votes) })}
                </Text>
              </Pressable>
            ))}
            {!state.decided ? (
              <NativeButton
                label={t('miniapps.closeSelection')}
                disabled={busy}
                onPress={() => onCall('hike_decide', {})}
              />
            ) : (
              <Text style={{ color: colors.accent }}>
                {t('miniapps.selectionClosed', { selection: text(state.decided) })}
              </Text>
            )}
          </Surface>
          {state.itinerary ? (
            <Surface>
              <Text style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}>
                {t('miniapps.itinerary')}
              </Text>
              {Object.entries(object(state.itinerary)).map(([key, value]) => (
                <Text key={key} selectable style={{ color: colors.secondary, lineHeight: 23 }}>
                  {key}: {typeof value === 'string' ? value : JSON.stringify(value)}
                </Text>
              ))}
            </Surface>
          ) : null}
        </>
      ) : null}
      {app.app_id === 'schedule' ? (
        <>
          <Surface>
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 24, fontWeight: '600' }}
            >
              {text(state.business) || text(state.title)}
            </Text>
            <Text style={{ color: colors.secondary }}>{text(state.timezone)}</Text>
            {list(state.services).map((service) => (
              <View key={text(service.id)} style={{ gap: 5, paddingVertical: 10 }}>
                <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
                  {text(service.name)}
                </Text>
                <Text style={{ color: colors.secondary }}>
                  {t('miniapps.serviceDurationAndPrice', {
                    minutes: String(service.minutes ?? '—'),
                    price: exactMoney(service.priceCents, state.currency),
                  })}
                </Text>
              </View>
            ))}
          </Surface>
          <Surface>
            <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>
              {t('miniapps.yourBookings')}
            </Text>
            {list(state.bookings).length ? (
              list(state.bookings).map((booking) => (
                <View key={text(booking.id)} style={{ gap: 7, paddingVertical: 12 }}>
                  <Text style={{ color: colors.text, fontWeight: '600' }}>
                    {text(booking.service) || text(booking.serviceName) || text(booking.serviceId)}
                  </Text>
                  <Text style={{ color: colors.secondary }}>
                    {time(booking.startMs, state.timezone)} · {text(booking.status)}
                  </Text>
                  {text(booking.forName) ? (
                    <Text style={{ color: colors.secondary }}>{text(booking.forName)}</Text>
                  ) : null}
                  {booking.status !== 'cancelled' ? (
                    <NativeButton
                      secondary
                      label={t('miniapps.requestCancellation')}
                      disabled={busy}
                      onPress={() => onCall('schedule_cancel', { bookingId: text(booking.id) })}
                    />
                  ) : null}
                </View>
              ))
            ) : (
              <Text style={{ color: colors.secondary }}>{t('miniapps.noBookingsInThisView')}</Text>
            )}
          </Surface>
        </>
      ) : null}
      {app.app_id === 'maptap' ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>MapTap</Text>
          <Text style={{ color: colors.secondary }}>
            {t('miniapps.theGroupSharesTheGameLocationsEnterYour')}
          </Text>
          {list(state.places).map((place, index) => (
            <Text key={index} style={{ color: colors.text }}>
              {t('miniapps.roundLocation', {
                position: index + 1,
                place: text(place.name) || text(place.title),
              })}
            </Text>
          ))}
        </Surface>
      ) : null}
      {list(state.log).length ? (
        <Surface>
          <Text style={{ color: colors.text, fontWeight: '600' }}>
            {t('miniapps.whatHappened')}
          </Text>
          {list(state.log)
            .slice(0, 8)
            .map((entry, index) => (
              <Text key={index} style={{ color: colors.secondary, lineHeight: 21 }}>
                {text(entry.who)} {text(entry.what)}
              </Text>
            ))}
        </Surface>
      ) : null}
    </View>
  );
}
