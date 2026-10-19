import { useEffect, useState } from 'react';
import { Stat, StatGrid } from '@zoen/ui';
import { Empty, Section, Shell, useBuiltin } from './shared';

type CountdownState = {
  title: string;
  place: string | null;
  target: number | string;
  log: { who: string; what: string; at: number }[];
};

function targetMilliseconds(value: CountdownState['target']): number | null {
  if (typeof value === 'string' && !/^-?\d+$/.test(value)) return null;
  const result = typeof value === 'number' ? value : Number(value);
  return Number.isSafeInteger(result) && Math.abs(result) <= 8_640_000_000_000_000 ? result : null;
}

export function Countdown() {
  const { state, busy, error, L } = useBuiltin<CountdownState>();
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  if (!state) {
    return (
      <Shell title={L('Contagem', 'Countdown')} busy={busy} error={error}>
        <Empty>{L('Carregando a contagem…', 'Loading the countdown…')}</Empty>
      </Shell>
    );
  }

  const target = targetMilliseconds(state.target);
  if (target === null) {
    return (
      <Shell title={state.title} busy={busy} error={error}>
        <Empty>
          {L(
            'Esta data não pode ser exibida neste dispositivo.',
            'This date cannot be displayed on this device.',
          )}
        </Empty>
      </Shell>
    );
  }

  const remaining = Math.max(0, Math.floor((target - now) / 1000));
  const date = new Intl.DateTimeFormat(L('pt-BR', 'en'), {
    dateStyle: 'full',
    timeStyle: 'short',
  }).format(target);
  const last = state.log[0];

  return (
    <Shell title={state.title} busy={busy} error={error}>
      <section
        className="hero stack"
        style={{
          background: 'linear-gradient(145deg, #f7bf88, #d08b81 58%, #6c99b3)',
          color: '#172d3a',
        }}
      >
        <span aria-hidden="true" style={{ fontSize: 52 }}>
          ☀︎
        </span>
        <h2 style={{ margin: 0, fontSize: 'clamp(28px, 8vw, 44px)' }}>
          {state.place || state.title}
        </h2>
        <time dateTime={new Date(target).toISOString()}>{date}</time>
      </section>
      <Section
        title={
          remaining === 0 ? L('Chegou o dia', 'The time has arrived') : L('Faltam', 'Time to go')
        }
      >
        <div role="timer" aria-label={L('Tempo restante', 'Time remaining')}>
          <StatGrid>
            <Stat value={Math.floor(remaining / 86400)} label={L('dias', 'days')} />
            <Stat value={Math.floor((remaining % 86400) / 3600)} label={L('horas', 'hours')} />
            <Stat value={Math.floor((remaining % 3600) / 60)} label={L('min', 'min')} />
            <Stat value={remaining % 60} label={L('seg', 'sec')} />
          </StatGrid>
        </div>
        <p className="muted">
          {L(
            'Data e horário locais deste dispositivo.',
            'Date and time use this device’s time zone.',
          )}
        </p>
      </Section>
      {last && (
        <p className="muted">
          {last.who} {last.what}
        </p>
      )}
    </Shell>
  );
}

export default Countdown;
