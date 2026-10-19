import { useEffect, useRef, useState } from 'react';
import { Pill } from '@zoen/ui';
import {
  Activity,
  ActionForm,
  Empty,
  Refresh,
  Section,
  Shell,
  useBuiltin,
  type LogEntry,
} from './shared';
import { newDash, jumpDash, stepDash, type DashState } from './dash';
import { Sprite } from './sprite';
type PetState = {
  name: string;
  fullness: number;
  energy: number;
  joy: number;
  mood: string;
  asleep: boolean;
  released: boolean;
  log: LogEntry[];
  dash: { runs: number; best: Record<string, { meters: number; carrots: number }> };
};
export function Pet() {
  const { state, busy, error, L, call } = useBuiltin<PetState>();
  const [name, setName] = useState('');
  const [tab, setTab] = useState<'care' | 'play' | 'board'>('care');
  if (!state)
    return (
      <Shell title={L('Bichinho do grupo', 'Group pet')}>
        <Empty>{L('Abrindo o bichinho…', 'Opening the pet…')}</Empty>
      </Shell>
    );
  const selectedTab = state.released && tab === 'play' ? 'care' : tab;
  const rename = () => {
    const captured = name.trim();
    if (captured)
      void call('pet_rename', { name: captured }).then((ok) => {
        if (ok) setName((current) => (current.trim() === captured ? '' : current));
      });
  };
  const tabs = [
    ['care', L('Cuidar', 'Care')],
    ['play', L('Brincar', 'Play')],
    ['board', L('Placar', 'Leaderboard')],
  ] as const;
  return (
    <Shell
      title={state.name}
      subtitle={
        state.released ? L('O bichinho foi devolvido.', 'The pet has been released.') : state.mood
      }
      busy={busy}
      error={error}
    >
      <div className="pet-tabs" role="tablist" aria-label={L('Bichinho do grupo', 'Group pet')}>
        {tabs.map(([id, label]) => (
          <button
            key={id}
            id={`pet-tab-${id}`}
            role="tab"
            aria-selected={selectedTab === id}
            aria-controls={`pet-panel-${id}`}
            tabIndex={selectedTab === id ? 0 : -1}
            disabled={id === 'play' && state.released}
            onClick={() => setTab(id)}
            onKeyDown={(event) => {
              const direction = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
              const enabled = tabs.filter(([key]) => key !== 'play' || !state.released);
              const index = enabled.findIndex(([key]) => key === id);
              const next =
                event.key === 'Home'
                  ? 0
                  : event.key === 'End'
                    ? enabled.length - 1
                    : direction
                      ? (index + direction + enabled.length) % enabled.length
                      : -1;
              if (next < 0) return;
              event.preventDefault();
              const key = enabled[next][0];
              setTab(key);
              document.getElementById(`pet-tab-${key}`)?.focus();
            }}
          >
            {label}
          </button>
        ))}
      </div>
      <div
        id="pet-panel-care"
        role="tabpanel"
        aria-labelledby="pet-tab-care"
        hidden={selectedTab !== 'care'}
        className="stack"
      >
        <Section>
          <figure className="pet-art">
            <figcaption className="sr-only">
              {state.released
                ? L('Pastinho vazio', 'Empty pasture')
                : state.asleep
                  ? L('Jumento dormindo', 'Sleeping donkey')
                  : L('Jumento do grupo', 'Group donkey')}
            </figcaption>
            <svg className="donkey" viewBox="0 0 160 96" aria-hidden="true">
              <rect width="160" height="96" fill="var(--z-muted)" />
              <circle cx="136" cy="18" r="8" fill="#e9c36b" />
              <rect y="74" width="160" height="22" fill="#a6c786" />
              {!state.released && <Sprite x={50} y={30} scale={2} asleep={state.asleep} />}
            </svg>
          </figure>
          <div className="stack">
            {[
              [L('Fome saciada', 'Fullness'), state.fullness],
              [L('Energia', 'Energy'), state.energy],
              [L('Alegria', 'Joy'), state.joy],
            ].map(([label, value]) => (
              <label className="meter" key={String(label)}>
                <span>{label}</span>
                <progress max={100} value={Number(value)} aria-label={String(label)} />
                <b>{Math.round(Number(value))}</b>
              </label>
            ))}
          </div>
          {!state.released && (
            <div className="row">
              <Pill disabled={busy || state.asleep} onClick={() => void call('pet_feed')}>
                {L('Dar cenoura', 'Feed a carrot')}
              </Pill>
              <Pill
                tone="glass"
                disabled={busy}
                onClick={() => void call(state.asleep ? 'pet_wake' : 'pet_nap')}
              >
                {state.asleep ? L('Acordar', 'Wake up') : L('Cochilar', 'Nap')}
              </Pill>
              <Pill
                tone="glass"
                disabled={busy || state.asleep}
                onClick={() => void call('pet_play')}
              >
                {L('Fazer carinho', 'Give affection')}
              </Pill>
              <Pill tone="glass" disabled={busy || state.asleep} onClick={() => setTab('play')}>
                Donkey Dash
              </Pill>
            </div>
          )}
        </Section>
        {!state.released && (
          <>
            <Section title={L('Nome do bichinho', 'Pet name')}>
              <ActionForm onSubmit={rename}>
                <label className="field">
                  {L('Novo nome', 'New name')}
                  <input
                    value={name}
                    maxLength={18}
                    disabled={busy}
                    onChange={(event) => setName(event.target.value)}
                  />
                </label>
                <Pill disabled={busy || !name.trim()} onClick={rename}>
                  {L('Renomear', 'Rename')}
                </Pill>
              </ActionForm>
            </Section>
            <Section>
              <button className="danger" disabled={busy} onClick={() => void call('pet_release')}>
                {L('Devolver o bichinho…', 'Release the pet…')}
              </button>
              <p className="muted">
                {L(
                  'O Zoen pede confirmação antes de encerrar para todo o grupo.',
                  'Zoen asks for confirmation before ending it for the group.',
                )}
              </p>
            </Section>
          </>
        )}
      </div>
      {!state.released && (
        <div
          id="pet-panel-play"
          role="tabpanel"
          aria-labelledby="pet-tab-play"
          hidden={selectedTab !== 'play'}
        >
          <Dash
            active={selectedTab === 'play'}
            busy={busy}
            asleep={state.asleep}
            onScore={(meters, carrots) => call('pet_dash_score', { meters, carrots })}
            L={L}
          />
        </div>
      )}
      <div
        id="pet-panel-board"
        role="tabpanel"
        aria-labelledby="pet-tab-board"
        hidden={selectedTab !== 'board'}
      >
        <Section title={L('Placar do grupo', 'Group leaderboard')}>
          <p className="muted">
            {state.dash.runs} {L('corridas', 'runs')}
          </p>
          {Object.entries(state.dash.best)
            .sort((a, b) => b[1].meters - a[1].meters)
            .map(([who, score]) => (
              <div className="board-row" key={who}>
                <span>{who}</span>
                <b>
                  {score.meters} m · {score.carrots} 🥕
                </b>
              </div>
            ))}
          {Object.keys(state.dash.best).length === 0 && (
            <Empty>{L('A primeira corrida pode ser sua.', 'You can run the first race.')}</Empty>
          )}
        </Section>
      </div>
      <Activity entries={state.log} L={L} />
      <Refresh tool="pet_get_state" busy={busy} onCall={call} L={L} />
    </Shell>
  );
}
function Dash({
  active,
  busy,
  asleep,
  onScore,
  L,
}: {
  active: boolean;
  busy: boolean;
  asleep: boolean;
  onScore: (meters: number, carrots: number) => Promise<boolean>;
  L: (pt: string, en: string) => string;
}) {
  const game = useRef(newDash());
  const [phase, setPhase] = useState<'ready' | 'running' | 'paused' | 'over'>('ready'),
    [display, setDisplay] = useState(newDash),
    [saved, setSaved] = useState(false);
  const blocked = !active || asleep || busy;
  const [previousBlocked, setPreviousBlocked] = useState(blocked);
  if (blocked !== previousBlocked) {
    setPreviousBlocked(blocked);
    if (blocked && phase === 'running') setPhase('paused');
  }
  useEffect(() => {
    if (phase !== 'running' || blocked) return;
    let active = true,
      frame = 0;
    const tick = (now: number) => {
      if (!active) return;
      const ended = stepDash(game.current, now, 360);
      setDisplay(dashSnapshot(game.current));
      if (ended) {
        setPhase('over');
        return;
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
      active = false;
      cancelAnimationFrame(frame);
    };
  }, [phase, blocked]);
  useEffect(() => {
    const pause = () => {
      if (document.visibilityState === 'hidden') {
        setPhase((current) => (current === 'running' ? 'paused' : current));
        game.current.last = null;
      }
    };
    document.addEventListener('visibilitychange', pause);
    return () => document.removeEventListener('visibilitychange', pause);
  }, []);
  const jump = () => {
    if (active && phase === 'running') jumpDash(game.current);
  };
  const g = display;
  return (
    <Section title="Donkey Dash">
      <p id="dash-instructions" className="muted">
        {L(
          'Pule as cercas e pegue cenouras. Você pode pausar a qualquer momento.',
          'Jump over fences and collect carrots. You can pause at any time.',
        )}
      </p>
      <button
        className="dash-board"
        onClick={jump}
        disabled={phase !== 'running'}
        aria-label={L('Pular', 'Jump')}
        aria-describedby="dash-instructions"
      >
        <svg viewBox="0 0 360 190" aria-hidden="true">
          <line x1="0" y1="150" x2="360" y2="150" stroke="var(--z-action)" strokeWidth="3" />
          <Sprite x={42} y={106 - g.y} scale={2} />
          {g.obstacles.map((x, i) => (
            <rect key={i} x={x} y={124} width={12} height={26} rx={2} fill="#a58d71" />
          ))}
          {g.carrotsX.map((c, i) => (
            <text key={i} x={c.x} y={150 - c.h} fontSize={16}>
              🥕
            </text>
          ))}
        </svg>
      </button>
      <p className="hero">
        {Math.floor(g.distance)} m · {g.carrots} 🥕
      </p>
      <div className="row">
        {(phase === 'ready' || phase === 'over') && (
          <Pill
            disabled={!active || busy || asleep}
            onClick={() => {
              if (!active || busy || asleep) return;
              game.current = newDash();
              setDisplay(dashSnapshot(game.current));
              setSaved(false);
              setPhase('running');
            }}
          >
            {L('Começar corrida', 'Start run')}
          </Pill>
        )}
        {phase === 'running' && (
          <Pill
            tone="glass"
            onClick={() => {
              game.current.last = null;
              setPhase('paused');
            }}
          >
            {L('Pausar', 'Pause')}
          </Pill>
        )}
        {phase === 'paused' && (
          <>
            <Pill
              disabled={!active || busy || asleep}
              onClick={() => {
                if (!active || busy || asleep) return;
                game.current.last = null;
                setPhase('running');
              }}
            >
              {L('Continuar', 'Resume')}
            </Pill>
            <Pill tone="glass" disabled={busy} onClick={() => setPhase('over')}>
              {L('Terminar corrida', 'Finish run')}
            </Pill>
          </>
        )}
        {phase === 'over' && (
          <Pill
            disabled={busy || saved}
            onClick={() =>
              void onScore(Math.floor(g.distance), g.carrots).then((ok) => {
                if (ok) setSaved(true);
              })
            }
          >
            {saved ? L('Resultado salvo', 'Result saved') : L('Salvar resultado', 'Save result')}
          </Pill>
        )}
      </div>
      <output className="sr-only">
        {phase === 'over'
          ? L(
              `Corrida encerrada: ${Math.floor(g.distance)} metros e ${g.carrots} cenouras.`,
              `Run finished: ${Math.floor(g.distance)} meters and ${g.carrots} carrots.`,
            )
          : phase === 'paused'
            ? L('Corrida pausada', 'Run paused')
            : ''}
      </output>
    </Section>
  );
}
function dashSnapshot(value: DashState): DashState {
  return {
    ...value,
    obstacles: [...value.obstacles],
    carrotsX: value.carrotsX.map((carrot) => ({ ...carrot })),
  };
}
