import { Pressable, Text, View } from 'react-native';
import { NativeButton, Surface } from '../components/primitives';
import { money } from '../core/presentation';
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
function exactMoney(value: unknown): string {
  if (typeof value === 'string' && /^-?\d+$/.test(value)) return money(value);
  if (typeof value === 'number' && Number.isSafeInteger(value)) return money(String(value));
  return 'Valor indisponível';
}
function time(value: unknown, zone?: unknown): string {
  const milliseconds = typeof value === 'string' && /^\d+$/.test(value) ? Number(value) : value;
  if (typeof milliseconds !== 'number' || !Number.isSafeInteger(milliseconds))
    return 'Data indisponível';
  try {
    return new Date(milliseconds).toLocaleString(
      'pt-BR',
      typeof zone === 'string' ? { timeZone: zone } : undefined,
    );
  } catch {
    return 'Data indisponível';
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
        O estado deste miniapp não pôde ser aberto. Atualize a página.
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
                label="Dar comida"
                disabled={busy || state.asleep === true}
                onPress={() => onCall('pet_feed', {})}
              />
              <NativeButton
                secondary
                label={state.asleep ? 'Acordar' : 'Cochilar'}
                disabled={busy}
                onPress={() => onCall(state.asleep ? 'pet_wake' : 'pet_nap', {})}
              />
              <NativeButton
                secondary
                label="Brincar"
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
              accessibilityLabel={`Votar em ${text(option.text)}, ${count(option.votes)} voto(s)`}
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
                {count(option.votes)} voto(s)
              </Text>
            </Pressable>
          ))}
          {state.closed ? (
            <Text style={{ color: colors.accent, fontWeight: '600' }}>
              Decidido: {text(state.winner)}
            </Text>
          ) : (
            <NativeButton
              secondary
              label="Fechar votação e levar ao plano"
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
              Porções: {String(state.servings ?? '—')}
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
                    : ` · ${typeof ingredient.qty === 'number' && typeof state.servings === 'number' && typeof state.baseServings === 'number' && state.baseServings > 0 ? ((ingredient.qty * state.servings) / state.baseServings).toLocaleString('pt-BR', { maximumFractionDigits: 2 }) : String(ingredient.qty)} ${text(ingredient.unit)}`}
                </Text>
              </Pressable>
            ))}
          </Surface>
          {Array.isArray(state.steps) ? (
            <Surface>
              <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>
                Modo cozinhar
              </Text>
              {state.steps.map((step, index) => (
                <Pressable
                  key={index}
                  disabled={busy}
                  accessibilityRole="button"
                  onPress={() => onCall('recipe_cook', { step: index })}
                  style={{ minHeight: 48, gap: 6, paddingVertical: 10 }}
                >
                  <Text style={{ color: colors.accent, fontWeight: '600' }}>Passo {index + 1}</Text>
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
                <Text style={{ color: colors.secondary }}>{count(trail.votes)} voto(s)</Text>
              </Pressable>
            ))}
            {!state.decided ? (
              <NativeButton
                label="Fechar escolha"
                disabled={busy}
                onPress={() => onCall('hike_decide', {})}
              />
            ) : (
              <Text style={{ color: colors.accent }}>Escolha fechada: {text(state.decided)}</Text>
            )}
          </Surface>
          {state.itinerary ? (
            <Surface>
              <Text style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}>Roteiro</Text>
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
                  {String(service.minutes ?? '—')} minutos · {exactMoney(service.priceCents)}
                </Text>
              </View>
            ))}
          </Surface>
          <Surface>
            <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>
              Seus agendamentos
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
                      label="Pedir cancelamento"
                      disabled={busy}
                      onPress={() => onCall('schedule_cancel', { bookingId: text(booking.id) })}
                    />
                  ) : null}
                </View>
              ))
            ) : (
              <Text style={{ color: colors.secondary }}>
                Nenhum agendamento nesta visualização.
              </Text>
            )}
          </Surface>
        </>
      ) : null}
      {app.app_id === 'maptap' ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>MapTap</Text>
          <Text style={{ color: colors.secondary }}>
            Os lugares da partida são compartilhados pelo grupo. Registre seu palpite em “Cravar
            palpite”.
          </Text>
          {list(state.places).map((place, index) => (
            <Text key={index} style={{ color: colors.text }}>
              Rodada {index + 1}: {text(place.name) || text(place.title)}
            </Text>
          ))}
        </Surface>
      ) : null}
      {list(state.log).length ? (
        <Surface>
          <Text style={{ color: colors.text, fontWeight: '600' }}>O que aconteceu</Text>
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
