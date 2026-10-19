import { useEffect, useMemo, useRef, useState, type PointerEvent } from 'react';
import { Pill, Stat, StatGrid, useMembers } from '@zoen/ui';
import { Empty, Section, Shell, useBuiltin } from './shared';
import {
  arcPath,
  canvasPoint,
  GLOBE_RADIUS,
  GLOBE_SIDE,
  graticulePath,
  inverse,
  landPath,
  rotate,
  slerp,
  type GeoPoint,
} from './globe';
import land from './land110.json';

type Point = GeoPoint;
type Guess = Point & { km: number; points: number };
type MapTapState = {
  places: (Point & { name: string; country: string; hint: string })[];
  guesses: Record<string, Record<string, Guess>>;
  order: string[];
  log: { who: string; what: string; at: number }[];
};

export function MapTap() {
  const { state, busy, error, call, L } = useBuiltin<MapTapState>();
  const members = useMembers();
  const me = members.find((member) => member.isMe);
  const [selectedRound, setSelectedRound] = useState<number | null>(null);
  const [latitude, setLatitude] = useState('0');
  const [longitude, setLongitude] = useState('0');
  const [hasPick, setHasPick] = useState(false);
  const submitting = useRef(false);

  if (!state) {
    return (
      <Shell title="MapTap" busy={busy} error={error}>
        <Empty>{L('Carregando a partida…', 'Loading the game…')}</Empty>
      </Shell>
    );
  }

  const myGuesses = me && Object.hasOwn(state.guesses, me.name) ? state.guesses[me.name] : {};
  const nextRound = state.places.findIndex((_, index) => !Object.hasOwn(myGuesses, String(index)));
  const round =
    selectedRound !== null && selectedRound < state.places.length
      ? selectedRound
      : nextRound >= 0
        ? nextRound
        : Math.max(0, state.places.length - 1);
  const place = state.places[round];
  const result = Object.hasOwn(myGuesses, String(round)) ? myGuesses[String(round)] : null;
  const lat = Number(latitude);
  const lon = Number(longitude);
  const validPick =
    hasPick &&
    latitude.trim() !== '' &&
    longitude.trim() !== '' &&
    Number.isFinite(lat) &&
    Number.isFinite(lon) &&
    lat >= -90 &&
    lat <= 90 &&
    lon >= -180 &&
    lon <= 180;
  const pick = result ?? (validPick ? { lat, lon } : null);
  const score = Object.values(myGuesses).reduce((sum, guess) => sum + guess.points, 0);
  const board = Object.entries(state.guesses)
    .map(([name, guesses]) => ({
      name,
      score: Object.values(guesses).reduce((sum, guess) => sum + guess.points, 0),
      rounds: Object.keys(guesses).length,
    }))
    .sort((a, b) => b.score - a.score || state.order.indexOf(a.name) - state.order.indexOf(b.name));

  const lock = async () => {
    if (!me || !place || !validPick || result || busy || submitting.current) return;
    submitting.current = true;
    setSelectedRound(round);
    try {
      await call('maptap_guess', { round, lat, lon });
    } finally {
      submitting.current = false;
    }
  };
  const changeRound = (index: number) => {
    if (busy || submitting.current) return;
    setSelectedRound(index);
    setHasPick(false);
    setLatitude('0');
    setLongitude('0');
  };

  return (
    <Shell
      title="MapTap"
      subtitle={L('Os mesmos lugares para todo o grupo.', 'The same places for the whole group.')}
      busy={busy}
      error={error}
    >
      <StatGrid>
        <Stat
          value={score.toLocaleString(L('pt-BR', 'en'))}
          label={L('seus pontos', 'your points')}
        />
        <Stat
          value={`${Object.keys(myGuesses).length}/${state.places.length}`}
          label={L('rodadas cravadas', 'rounds locked in')}
        />
      </StatGrid>
      <fieldset
        className="row"
        aria-label={L('Rodadas', 'Rounds')}
        style={{ flexWrap: 'wrap', border: 0, margin: 0, padding: 0, minWidth: 0 }}
      >
        {state.places.map((_, index) => (
          <Pill
            key={index}
            tone={index === round ? 'black' : 'glass'}
            disabled={busy}
            label={`${L('Rodada', 'Round')} ${index + 1}${Object.hasOwn(myGuesses, String(index)) ? ` · ${L('cravada', 'locked')}` : ''}`}
            onClick={() => changeRound(index)}
          >
            {index + 1}
            {Object.hasOwn(myGuesses, String(index)) ? ' ✓' : ''}
          </Pill>
        ))}
      </fieldset>
      {place ? (
        <Section title={`${L('Rodada', 'Round')} ${round + 1} · ${place.name}`}>
          <p>{place.hint}</p>
          <Globe
            key={`${round}:${result ? 'result' : 'pick'}`}
            pick={pick}
            actual={result ? place : null}
            disabled={busy}
            onPick={(point) => {
              if (result || busy || submitting.current) return;
              setLatitude(point.lat.toFixed(4));
              setLongitude(point.lon.toFixed(4));
              setHasPick(true);
            }}
            L={L}
          />
          {result ? (
            <div className="stack">
              <StatGrid>
                <Stat
                  value={`${result.km.toLocaleString(L('pt-BR', 'en'))} km`}
                  label={L('do destino', 'from the destination')}
                />
                <Stat
                  value={result.points.toLocaleString(L('pt-BR', 'en'))}
                  label={L('pontos nesta rodada', 'points this round')}
                />
              </StatGrid>
              <p>
                {place.name}, {place.country} · {place.lat.toFixed(2)}°, {place.lon.toFixed(2)}°
              </p>
              <p className="muted">
                {L(
                  'Turquesa: seu palpite. Dourado: destino.',
                  'Turquoise: your guess. Gold: destination.',
                )}
              </p>
              {nextRound >= 0 && (
                <Pill disabled={busy} onClick={() => changeRound(nextRound)}>
                  {L('Próxima rodada', 'Next round')}
                </Pill>
              )}
            </div>
          ) : (
            <div className="stack">
              <p className="muted">
                {L(
                  'Arraste o globo para girar e toque para escolher. Você também pode informar as coordenadas. Norte e leste são positivos.',
                  'Drag the globe to rotate and tap to choose. You can also enter coordinates. North and east are positive.',
                )}
              </p>
              <div className="row">
                <label className="stack" style={{ flex: 1 }}>
                  {L('Latitude', 'Latitude')}
                  <input
                    className="field"
                    type="number"
                    inputMode="decimal"
                    min={-90}
                    max={90}
                    step="any"
                    value={latitude}
                    disabled={busy}
                    onChange={(event) => {
                      setLatitude(event.target.value);
                      setHasPick(true);
                    }}
                  />
                </label>
                <label className="stack" style={{ flex: 1 }}>
                  {L('Longitude', 'Longitude')}
                  <input
                    className="field"
                    type="number"
                    inputMode="decimal"
                    min={-180}
                    max={180}
                    step="any"
                    value={longitude}
                    disabled={busy}
                    onChange={(event) => {
                      setLongitude(event.target.value);
                      setHasPick(true);
                    }}
                  />
                </label>
              </div>
              {hasPick && !validPick && (
                <p role="alert">
                  {L(
                    'Latitude de −90 a 90 e longitude de −180 a 180.',
                    'Use latitude −90 to 90 and longitude −180 to 180.',
                  )}
                </p>
              )}
              {!me && (
                <output>
                  {L('Aguardando sua identificação no grupo…', 'Waiting for your group identity…')}
                </output>
              )}
              <Pill
                disabled={busy || !me || !validPick}
                onClick={() => {
                  void lock();
                }}
              >
                {L('Cravar palpite', 'Lock in guess')}
              </Pill>
            </div>
          )}
        </Section>
      ) : (
        <Empty>
          {L('Esta partida não tem lugares disponíveis.', 'This game has no available places.')}
        </Empty>
      )}
      <Section title={L('Placar do grupo', 'Group leaderboard')}>
        {board.length ? (
          <ol className="list">
            {board.map((row) => (
              <li className="item row" key={row.name}>
                <strong style={{ flex: 1 }}>{row.name}</strong>
                <span>
                  {row.score.toLocaleString(L('pt-BR', 'en'))} pts · {row.rounds}/
                  {state.places.length}
                </span>
              </li>
            ))}
          </ol>
        ) : (
          <Empty>
            {L('O primeiro palpite abre o placar.', 'The first guess starts the leaderboard.')}
          </Empty>
        )}
      </Section>
    </Shell>
  );
}

export default MapTap;

type GlobeProps = {
  pick: GeoPoint | null;
  actual: GeoPoint | null;
  disabled: boolean;
  onPick: (point: GeoPoint) => void;
  L: (pt: string, en: string) => string;
};

function Globe({ pick, actual, disabled, onPick, L }: GlobeProps) {
  const [center, setCenter] = useState<GeoPoint>(() =>
    pick && actual ? slerp(pick, actual, 0.5) : { lat: 25, lon: 10 },
  );
  const gesture = useRef<{
    id: number;
    x: number;
    y: number;
    side: number;
    center: GeoPoint;
    moved: boolean;
  } | null>(null);
  useEffect(() => {
    if (disabled) gesture.current = null;
  }, [disabled]);

  // Natural Earth 1:110m, public domain; byte-identical to the existing native
  // apple/Shared/Resources/land110.json. No map/network request is involved.
  const continents = useMemo(() => landPath(land, center), [center]);
  const grid = useMemo(() => graticulePath(center), [center]);
  const line = pick && actual ? arcPath(pick, actual, center) : '';
  const marker = pick ? canvasPoint(pick, center) : null;
  const target = actual ? canvasPoint(actual, center) : null;

  const begin = (event: PointerEvent<SVGSVGElement>) => {
    if (disabled || event.button !== 0 || event.isPrimary === false || gesture.current) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const side = Math.min(bounds.width, bounds.height);
    if (side <= 0) return;
    gesture.current = {
      id: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      side,
      center,
      moved: false,
    };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const move = (event: PointerEvent<SVGSVGElement>) => {
    const active = gesture.current;
    if (!active || active.id !== event.pointerId || disabled) return;
    const dx = event.clientX - active.x,
      dy = event.clientY - active.y;
    if (!active.moved && Math.hypot(dx, dy) < 6) return;
    active.moved = true;
    setCenter(rotate(active.center, dx, dy, active.side));
  };
  const end = (event: PointerEvent<SVGSVGElement>) => {
    const active = gesture.current;
    if (!active || active.id !== event.pointerId) return;
    gesture.current = null;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    if (
      active.moved ||
      Math.hypot(event.clientX - active.x, event.clientY - active.y) >= 6 ||
      disabled ||
      actual
    )
      return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const radius = Math.min(bounds.width, bounds.height) * 0.47;
    if (radius <= 0) return;
    const point = inverse(
      (event.clientX - bounds.left - bounds.width / 2) / radius,
      -(event.clientY - bounds.top - bounds.height / 2) / radius,
      active.center,
    );
    if (point) onPick(point);
  };
  const turn = (dx: number, dy: number) => {
    if (!disabled) setCenter((current) => rotate(current, dx, dy, GLOBE_SIDE));
  };

  return (
    <fieldset
      className="stack"
      aria-label={L('Globo giratório', 'Rotatable globe')}
      style={{ border: 0, margin: 0, padding: 0, minWidth: 0 }}
    >
      <svg
        viewBox={`0 0 ${GLOBE_SIDE} ${GLOBE_SIDE}`}
        onPointerDown={begin}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={() => {
          gesture.current = null;
        }}
        onLostPointerCapture={() => {
          gesture.current = null;
        }}
        data-globe="orthographic"
        aria-hidden="true"
        style={{
          width: '100%',
          height: 'auto',
          display: 'block',
          aspectRatio: '1',
          borderRadius: 24,
          background: '#050f24',
          touchAction: 'none',
          cursor: disabled ? 'default' : 'grab',
        }}
      >
        <defs>
          <radialGradient id="maptap-ocean" cx="32%" cy="27%" r="80%">
            <stop offset="0" stopColor="#1d4e8f" />
            <stop offset="0.65" stopColor="#0b2752" />
            <stop offset="1" stopColor="#050f24" />
          </radialGradient>
          <linearGradient id="maptap-land" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="#9db07a" />
            <stop offset="1" stopColor="#d9c8a0" />
          </linearGradient>
          <radialGradient id="maptap-shade" cx="30%" cy="25%" r="85%">
            <stop offset="0" stopColor="white" stopOpacity="0.12" />
            <stop offset="0.45" stopColor="black" stopOpacity="0" />
            <stop offset="1" stopColor="black" stopOpacity="0.5" />
          </radialGradient>
          <clipPath id="maptap-disc">
            <circle cx="180" cy="180" r={GLOBE_RADIUS} />
          </clipPath>
        </defs>
        <circle cx="180" cy="180" r={GLOBE_RADIUS + 4} fill="#28537d" opacity="0.6" />
        <circle cx="180" cy="180" r={GLOBE_RADIUS} fill="url(#maptap-ocean)" />
        <g clipPath="url(#maptap-disc)">
          <path
            data-land="natural-earth-110m"
            d={continents}
            fill="url(#maptap-land)"
            stroke="#8fa36b"
            strokeWidth="0.6"
            strokeLinejoin="round"
          />
          <path d={grid} fill="none" stroke="white" strokeOpacity="0.16" strokeWidth="0.5" />
          <circle cx="180" cy="180" r={GLOBE_RADIUS} fill="url(#maptap-shade)" />
          {line && (
            <path
              data-result-arc="true"
              d={line}
              fill="none"
              stroke="white"
              strokeWidth="1.5"
              strokeDasharray="4 4"
            />
          )}
          {target && target.z >= 0 && (
            <circle
              data-marker="target"
              cx={target.x}
              cy={target.y}
              r="6"
              fill="#ffd60a"
              stroke="#302d1b"
              strokeWidth="1.2"
            />
          )}
          {marker && marker.z >= 0 && (
            <g data-marker="guess">
              <circle cx={marker.x} cy={marker.y} r="9" fill="#18c8d4" fillOpacity="0.45" />
              <circle
                cx={marker.x}
                cy={marker.y}
                r="5"
                fill="#7ff7ff"
                stroke="white"
                strokeWidth="1.6"
              />
            </g>
          )}
        </g>
      </svg>
      <fieldset
        className="row"
        aria-label={L('Girar o globo', 'Rotate the globe')}
        style={{ border: 0, margin: 0, padding: 0, minWidth: 0 }}
      >
        <Pill
          tone="glass"
          disabled={disabled}
          label={L('Girar para oeste', 'Rotate west')}
          onClick={() => turn(45, 0)}
        >
          ←
        </Pill>
        <Pill
          tone="glass"
          disabled={disabled}
          label={L('Girar para leste', 'Rotate east')}
          onClick={() => turn(-45, 0)}
        >
          →
        </Pill>
        <Pill
          tone="glass"
          disabled={disabled}
          label={L('Girar para norte', 'Rotate north')}
          onClick={() => turn(0, 45)}
        >
          ↑
        </Pill>
        <Pill
          tone="glass"
          disabled={disabled}
          label={L('Girar para sul', 'Rotate south')}
          onClick={() => turn(0, -45)}
        >
          ↓
        </Pill>
      </fieldset>
      <p className="muted">
        {L('Centro do globo', 'Globe center')}: {center.lat.toFixed(1)}°, {center.lon.toFixed(1)}°
      </p>
      <div className="row">
        {pick && (
          <Pill tone="glass" disabled={disabled} onClick={() => setCenter({ ...pick })}>
            {L('Ver meu palpite', 'Show my guess')}
          </Pill>
        )}
        {actual && (
          <Pill tone="glass" disabled={disabled} onClick={() => setCenter({ ...actual })}>
            {L('Ver destino', 'Show destination')}
          </Pill>
        )}
      </div>
    </fieldset>
  );
}
