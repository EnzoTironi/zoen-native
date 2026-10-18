import { useEffect, useRef, useState } from 'react';
import { Pill, Stat, StatGrid } from '@zoen/ui';
import { Empty, Section, Shell, useBuiltin } from './shared';

type RecipeState = {
  title: string;
  subtitle: string;
  baseServings: number;
  servings: number;
  ingredients: { id: string; name: string; qty: number; unit: string; done: boolean }[];
  steps: { text: string; minutes: number }[];
  cooking: { who: string; step: number; at: number } | null;
  log: { who: string; what: string; at: number }[];
};

function amount(quantity: number, unit: string): string {
  if (unit === 'ml') return `${Math.round(quantity / 10) * 10} ml`;
  const whole = Math.floor(quantity);
  const fraction = quantity - whole;
  const part =
    fraction < 0.13
      ? ''
      : fraction < 0.38
        ? '¼'
        : fraction < 0.63
          ? '½'
          : fraction < 0.88
            ? '¾'
            : '';
  const integer = fraction >= 0.88 ? whole + 1 : whole;
  const count = integer === 0 && part ? part : `${integer}${part}`;
  const plurals: Record<string, string> = { colher: 'colheres', maço: 'maços', bunch: 'bunches' };
  const plural = quantity > 1.13 ? (plurals[unit] ?? unit) : unit;
  return `${count} ${plural}`.trim();
}

export function Recipe() {
  const { state, busy, error, call, L } = useBuiltin<RecipeState>();
  const [cooking, setCooking] = useState(false);
  const [timerEnd, setTimerEnd] = useState<number | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  useEffect(() => {
    if (timerEnd === null) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [timerEnd]);

  if (!state) {
    return (
      <Shell title={L('Receita', 'Recipe')} busy={busy} error={error}>
        <Empty>{L('Carregando a receita…', 'Loading the recipe…')}</Empty>
      </Shell>
    );
  }

  const total = state.steps.length;
  const finished = state.cooking !== null && state.cooking.step >= total;
  const step = Math.min(Math.max(0, state.cooking?.step ?? 0), Math.max(0, total - 1));
  const current = state.steps[step];
  const completed = state.ingredients.filter((ingredient) => ingredient.done).length;
  const last = state.log[0];
  const timerLeft = timerEnd === null ? null : Math.max(0, Math.ceil((timerEnd - now) / 1000));
  const changeStep = async (next: number) => {
    if (busy || next < 0 || next > total) return;
    const accepted = await call('recipe_cook', { step: next });
    if (accepted && alive.current && next === total) setCooking(false);
  };
  const start = async () => {
    if (busy || total === 0) return;
    if (state.cooking && !finished) {
      setCooking(true);
      return;
    }
    const accepted = await call('recipe_cook', { step: 0 });
    if (accepted && alive.current) setCooking(true);
  };

  return (
    <Shell title={state.title} subtitle={state.subtitle} busy={busy} error={error}>
      <div
        className="hero row"
        style={{ background: 'linear-gradient(140deg, #fbe3c8, #f3bd92)', color: '#503b30' }}
      >
        <span aria-hidden="true" style={{ fontSize: 64 }}>
          ♨︎
        </span>
        <div className="stack">
          <strong>{L('À mesa, juntos', 'Around the table')}</strong>
          <span>
            {L('Ingredientes e passos compartilhados.', 'Shared ingredients and cooking steps.')}
          </span>
        </div>
      </div>
      <StatGrid>
        <Stat value={state.servings} label={L('porções', 'servings')} />
        <Stat
          value={`${completed}/${state.ingredients.length}`}
          label={L('ingredientes separados', 'ingredients ready')}
        />
      </StatGrid>
      <Section title={L('Porções', 'Servings')}>
        <div className="row">
          <Pill
            disabled={busy || state.servings <= 1}
            label={L('Uma porção a menos', 'One fewer serving')}
            onClick={() => {
              void call('recipe_servings', { servings: state.servings - 1 });
            }}
          >
            −
          </Pill>
          <strong aria-live="polite">{state.servings}</strong>
          <Pill
            disabled={busy || state.servings >= 12}
            label={L('Uma porção a mais', 'One more serving')}
            onClick={() => {
              void call('recipe_servings', { servings: state.servings + 1 });
            }}
          >
            +
          </Pill>
        </div>
      </Section>
      <Section title={L('Ingredientes', 'Ingredients')}>
        <div className="list">
          {state.ingredients.map((ingredient) => (
            <label
              key={ingredient.id}
              className="item row"
              style={{ width: '100%', minHeight: 48, textAlign: 'start', color: 'inherit' }}
            >
              <input
                type="checkbox"
                checked={ingredient.done}
                aria-checked={ingredient.done}
                disabled={busy}
                onChange={() => {
                  void call('recipe_check', { id: ingredient.id });
                }}
                style={{
                  width: 22,
                  height: 22,
                  minHeight: 22,
                  padding: 0,
                  margin: 0,
                  accentColor: 'var(--z-action)',
                }}
              />
              <span
                style={{ flex: 1, textDecoration: ingredient.done ? 'line-through' : undefined }}
              >
                {ingredient.name}
              </span>
              <span className="muted">
                {amount(
                  (ingredient.qty * state.servings) / Math.max(1, state.baseServings),
                  ingredient.unit,
                )}
              </span>
            </label>
          ))}
        </div>
      </Section>
      {state.cooking && (
        <p className="muted">
          {state.cooking.who} ·{' '}
          {finished
            ? L('Terminou de cozinhar', 'Finished cooking')
            : `${L('Passo', 'Step')} ${state.cooking.step + 1}/${total}`}
        </p>
      )}
      {cooking && !finished && current ? (
        <Section title={`${L('Passo', 'Step')} ${step + 1}/${total}`}>
          <p style={{ fontSize: 24, lineHeight: 1.4 }}>{current.text}</p>
          {current.minutes > 0 && (
            <Pill
              tone="glass"
              onClick={() => {
                const clock = Date.now();
                setNow(clock);
                setTimerEnd(clock + current.minutes * 60_000);
              }}
            >
              {L('Iniciar timer de', 'Start a timer for')} {current.minutes} min
            </Pill>
          )}
          <div className="row">
            <Pill
              tone="glass"
              disabled={busy || step === 0}
              onClick={() => {
                void changeStep(step - 1);
              }}
            >
              {L('Anterior', 'Previous')}
            </Pill>
            <Pill
              disabled={busy}
              onClick={() => {
                void changeStep(step + 1);
              }}
            >
              {step + 1 === total ? L('Concluir', 'Finish') : L('Próximo', 'Next')}
            </Pill>
          </div>
          <Pill tone="glass" onClick={() => setCooking(false)}>
            {L('Voltar aos ingredientes', 'Back to ingredients')}
          </Pill>
        </Section>
      ) : (
        <Pill
          disabled={busy || total === 0}
          onClick={() => {
            void start();
          }}
        >
          {state.cooking && !finished
            ? L('Continuar cozinhando', 'Continue cooking')
            : L('Começar a cozinhar', 'Start cooking')}
        </Pill>
      )}
      {timerLeft !== null && (
        <Section title={L('Timer da cozinha', 'Kitchen timer')}>
          <div className="row">
            <strong role="timer" style={{ fontSize: 32, fontVariantNumeric: 'tabular-nums' }}>
              {Math.floor(timerLeft / 60)}:{String(timerLeft % 60).padStart(2, '0')}
            </strong>
            {timerLeft === 0 && <output>{L('Pronto!', 'Done!')}</output>}
            <Pill tone="glass" onClick={() => setTimerEnd(null)}>
              {L('Parar timer', 'Stop timer')}
            </Pill>
          </div>
          <p className="muted">
            {L(
              'Timer local enquanto esta receita está aberta. Não cria um alarme do sistema.',
              'A local timer while this recipe is open. It does not set a system alarm.',
            )}
          </p>
        </Section>
      )}
      {last && (
        <p className="muted">
          {last.who} {last.what}
        </p>
      )}
    </Shell>
  );
}

export default Recipe;
