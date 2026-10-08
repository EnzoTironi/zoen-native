// Trail "photos": painted in SVG, in Zoen's hand-drawn style (ink outline with a little
// wobble, flat paint, soft light). Drawn for this demo, so there's no licence to track,
// and they weigh a few KB instead of megabytes of JPEG.
import type { ReactElement } from 'react';
import type { TrailId } from './trails';

const INK = '#2B2530';

// Deterministic wobble so the same photo always draws the same way.
function rng(seed: number) { let s = seed >>> 0; return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32); }

/** A smooth path through points, nudged by a seeded wobble. */
function wob(pts: [number, number][], seed: number, amp = 1.6, close = false) {
  const r = rng(seed);
  const p = pts.map(([x, y]) => [x + (r() - 0.5) * amp * 2, y + (r() - 0.5) * amp * 2] as [number, number]);
  if (p.length < 3) return `M${p.map((q) => q.join(',')).join('L')}`;
  let d = `M${p[0][0].toFixed(1)},${p[0][1].toFixed(1)}`;
  for (let i = 1; i < p.length - 1; i++) {
    const [x1, y1] = p[i], [x2, y2] = p[i + 1];
    d += ` Q${x1.toFixed(1)},${y1.toFixed(1)} ${((x1 + x2) / 2).toFixed(1)},${((y1 + y2) / 2).toFixed(1)}`;
  }
  const last = p[p.length - 1];
  d += ` L${last[0].toFixed(1)},${last[1].toFixed(1)}`;
  return close ? d + ' Z' : d;
}

const Ink = ({ d, w = 2.2, o = 1 }: { d: string; w?: number; o?: number }) => <path d={d} fill="none" stroke={INK} strokeWidth={w} strokeLinecap="round" strokeLinejoin="round" opacity={o} />;
const Fill = ({ d, fill, ink = true, w = 2.2 }: { d: string; fill: string; ink?: boolean; w?: number }) => <path d={d} fill={fill} stroke={ink ? INK : 'none'} strokeWidth={w} strokeLinejoin="round" />;

function Sky({ id, top, bottom }: { id: string; top: string; bottom: string }) {
  return (<><defs><linearGradient id={id} x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor={top} /><stop offset="1" stopColor={bottom} /></linearGradient></defs><rect width="400" height="300" fill={`url(#${id})`} /></>);
}

function Waves({ y, n = 3, color = '#E8F4FF', seed = 1 }: { y: number; n?: number; color?: string; seed?: number }) {
  const r = rng(seed);
  return <>{Array.from({ length: n }, (_, i) => {
    const yy = y + i * 14; const x0 = r() * 60;
    const pts: [number, number][] = Array.from({ length: 6 }, (_, k) => [x0 + k * 70, yy + (k % 2 ? 4 : -2)]);
    return <path key={i} d={wob(pts, seed + i, 1)} fill="none" stroke={color} strokeWidth={2} strokeLinecap="round" opacity={0.8 - i * 0.2} />;
  })}</>;
}

function Elk({ x, y, s = 1, flip = false }: { x: number; y: number; s?: number; flip?: boolean }) {
  return (
    <g transform={`translate(${x},${y}) scale(${flip ? -s : s},${s})`}>
      <path d={wob([[-22, 0], [-18, -14], [6, -16], [16, -12], [18, 0]], 7, 0.6, true)} fill="#8A5A36" stroke={INK} strokeWidth={2} />
      <path d={wob([[14, -13], [22, -26], [28, -27], [26, -20], [18, -10]], 8, 0.5, true)} fill="#6E4628" stroke={INK} strokeWidth={2} />
      <Ink d={wob([[24, -27], [20, -40], [14, -46]], 9, 0.4)} w={1.8} />
      <Ink d={wob([[25, -27], [30, -40], [36, -46]], 10, 0.4)} w={1.8} />
      <Ink d={wob([[21, -36], [16, -36]], 11, 0.3)} w={1.6} />
      <Ink d={wob([[28, -36], [33, -35]], 12, 0.3)} w={1.6} />
      {[-16, -8, 8, 14].map((lx, i) => <Ink key={i} d={wob([[lx, -2], [lx + (i % 2 ? 1 : -1), 16]], 13 + i, 0.4)} w={2.2} />)}
      <path d="M-14,-12 Q-4,-9 6,-13" stroke="#C9A27A" strokeWidth={3} fill="none" opacity={0.7} />
    </g>
  );
}

function Redwood({ x, w, top = -10, seed }: { x: number; w: number; top?: number; seed: number }) {
  return (<g>
    <Fill d={wob([[x - w / 2, 310], [x - w / 2 + 2, 150], [x - w / 2 + 4, top]], seed, 1.2).replace(/^M/, 'M') + ` L${x + w / 2 - 4},${top} L${x + w / 2},310 Z`} fill="#6B3B26" />
    <Ink d={wob([[x - w / 6, 300], [x - w / 6 + 1, 120], [x - w / 6, 20]], seed + 1, 1)} w={1.4} o={0.5} />
    <Ink d={wob([[x + w / 5, 290], [x + w / 5, 160], [x + w / 5 - 1, 40]], seed + 2, 1)} w={1.4} o={0.5} />
  </g>);
}

function Fern({ x, y, s = 1, seed }: { x: number; y: number; s?: number; seed: number }) {
  const r = rng(seed);
  return <g transform={`translate(${x},${y}) scale(${s})`}>{Array.from({ length: 5 }, (_, i) => {
    const a = -150 + i * 30 + (r() - 0.5) * 10; const L = 34 + r() * 14;
    const ex = Math.cos((a * Math.PI) / 180) * L, ey = Math.sin((a * Math.PI) / 180) * L;
    return <path key={i} d={`M0,0 Q${ex * 0.5},${ey * 0.7 - 6} ${ex},${ey}`} stroke="#3F7D3A" strokeWidth={5} fill="none" strokeLinecap="round" />;
  })}</g>;
}

function Cypress({ x, y, s = 1, seed }: { x: number; y: number; s?: number; seed: number }) {
  return (<g transform={`translate(${x},${y}) scale(${s})`}>
    <Ink d={wob([[0, 0], [-4, -40], [6, -70], [-2, -92]], seed, 1)} w={6} />
    <Fill d={wob([[-50, -70], [-30, -100], [10, -112], [50, -104], [62, -84], [30, -74], [-10, -68]], seed + 1, 2, true)} fill="#2F5D3A" />
    <Fill d={wob([[-30, -40], [-6, -60], [26, -58], [34, -44], [4, -36]], seed + 2, 2, true)} fill="#3B7046" />
  </g>);
}

type Scene = () => ReactElement;

const scenes: Record<string, Scene> = {
  'tomales-0': () => (<>
    <Sky id="s1" top="#BFE0F5" bottom="#F7E6C8" />
    <Fill d={wob([[0, 150], [80, 140], [170, 146], [260, 138], [400, 144], [400, 210], [0, 210]], 3, 1, true)} fill="#3C6E9E" ink={false} />
    <Waves y={162} seed={4} />
    <Fill d={wob([[210, 150], [270, 118], [330, 112], [400, 120], [400, 150]], 5, 1.4, true)} fill="#8DAA6B" />
    <Fill d={wob([[0, 196], [90, 170], [190, 180], [300, 168], [400, 182], [400, 300], [0, 300]], 6, 1.6, true)} fill="#D9B45F" />
    <Fill d={wob([[0, 236], [120, 214], [260, 226], [400, 212], [400, 300], [0, 300]], 7, 1.6, true)} fill="#C49A45" />
    <path d={wob([[120, 300], [170, 250], [215, 214], [250, 190]], 8, 1)} stroke="#EBD9A9" strokeWidth={9} fill="none" strokeLinecap="round" />
    <Ink d={wob([[120, 300], [170, 250], [215, 214], [250, 190]], 9, 1)} w={1.4} o={0.4} />
    <circle cx={320} cy={58} r={22} fill="#FFE7A3" opacity={0.9} />
  </>),
  'tomales-1': () => (<>
    <Sky id="s2" top="#D9E4E8" bottom="#EEF1E6" />
    <Fill d={wob([[0, 140], [120, 124], [240, 132], [400, 118], [400, 180], [0, 180]], 21, 1.4, true)} fill="#9DB08A" />
    <Fill d={wob([[0, 170], [140, 160], [260, 168], [400, 156], [400, 300], [0, 300]], 22, 1.6, true)} fill="#B7B66A" />
    <Elk x={150} y={208} s={1.5} />
    <Elk x={268} y={190} s={1.05} flip />
    {Array.from({ length: 16 }, (_, i) => <path key={i} d={`M${12 + i * 25},${300} q2,-14 ${i % 2 ? 6 : -4},-22`} stroke="#8C8A3E" strokeWidth={2} fill="none" />)}
  </>),
  'tomales-2': () => (<>
    <Sky id="s3" top="#9CC6E8" bottom="#E3EEF5" />
    <Fill d={wob([[0, 120], [400, 112], [400, 300], [0, 300]], 31, 1, true)} fill="#2F6597" ink={false} />
    <Waves y={140} n={4} seed={32} />
    <Fill d={wob([[60, 300], [110, 214], [170, 180], [230, 172], [270, 196], [300, 250], [330, 300]], 33, 2, true)} fill="#A68A63" />
    <Fill d={wob([[130, 214], [180, 190], [226, 186], [250, 200], [200, 216]], 34, 1.6, true)} fill="#C2A77D" />
    <path d={wob([[40, 300], [70, 282], [110, 296], [150, 280], [200, 298]], 35, 2)} stroke="#fff" strokeWidth={6} fill="none" strokeLinecap="round" opacity={0.85} />
    <path d={wob([[250, 296], [290, 280], [340, 294], [400, 282]], 36, 2)} stroke="#fff" strokeWidth={6} fill="none" strokeLinecap="round" opacity={0.85} />
  </>),
  'steep-0': () => (<>
    <Sky id="s4" top="#C8DDC0" bottom="#6E8F5E" />
    <path d="M150,0 L250,0 L330,300 L120,300 Z" fill="#F4F0C8" opacity={0.35} />
    <Redwood x={50} w={60} seed={41} />
    <Redwood x={150} w={36} seed={42} />
    <Redwood x={300} w={70} seed={43} />
    <Redwood x={380} w={34} seed={44} />
    <Fill d={wob([[0, 250], [100, 236], [220, 246], [400, 232], [400, 300], [0, 300]], 45, 1.6, true)} fill="#45603A" />
    <Fern x={110} y={262} s={1.1} seed={46} /><Fern x={230} y={270} s={0.9} seed={47} /><Fern x={350} y={258} s={1.2} seed={48} />
  </>),
  'steep-1': () => (<>
    <Sky id="s5" top="#9DB894" bottom="#5D7A4F" />
    <Fill d={wob([[0, 0], [130, 0], [150, 120], [120, 300], [0, 300]], 51, 2, true)} fill="#6C7C66" />
    <Fill d={wob([[260, 0], [400, 0], [400, 300], [250, 300], [280, 150]], 52, 2, true)} fill="#5E6E58" />
    <path d={wob([[190, 0], [196, 90], [192, 180], [200, 300]], 53, 2)} stroke="#E9F6FF" strokeWidth={26} fill="none" strokeLinecap="round" opacity={0.9} />
    <path d={wob([[186, 10], [190, 120], [188, 290]], 54, 2)} stroke="#fff" strokeWidth={6} fill="none" opacity={0.8} />
    <g transform="translate(222,40) rotate(6)">
      <Ink d="M0,0 L0,240" w={5} /><Ink d="M36,0 L36,240" w={5} />
      {Array.from({ length: 9 }, (_, i) => <path key={i} d={`M0,${14 + i * 26} L36,${14 + i * 26}`} stroke="#B07A45" strokeWidth={6} />)}
      <path d="M0,0 L0,240 M36,0 L36,240" stroke="#9A6334" strokeWidth={3} />
    </g>
    <Fern x={70} y={290} s={1.2} seed={55} /><Fern x={340} y={292} s={1} seed={56} />
  </>),
  'steep-2': () => (<>
    <Sky id="s6" top="#B7CFA9" bottom="#7E9C6A" />
    <Redwood x={40} w={50} seed={61} /><Redwood x={360} w={56} seed={62} />
    <Fill d={wob([[0, 200], [400, 190], [400, 300], [0, 300]], 63, 1.4, true)} fill="#5F7F96" />
    <Waves y={214} n={3} color="#DCEBF5" seed={64} />
    {[[90, 250, 22], [170, 236, 16], [250, 258, 26], [320, 240, 14]].map(([x, y, r], i) => <ellipse key={i} cx={x} cy={y} rx={r} ry={r * 0.6} fill="#8C8E86" stroke={INK} strokeWidth={2} />)}
    <Fern x={130} y={198} s={0.9} seed={65} /><Fern x={290} y={196} s={1} seed={66} />
  </>),
  'lands-0': () => (<>
    <Sky id="s7" top="#F5C79A" bottom="#F9E7D2" />
    <Fill d={wob([[0, 170], [400, 160], [400, 300], [0, 300]], 71, 1, true)} fill="#4A77A3" ink={false} />
    <Waves y={190} n={3} seed={72} />
    <Fill d={wob([[230, 172], [290, 150], [400, 146], [400, 176]], 73, 1.4, true)} fill="#7C8A6C" />
    <g stroke="#C8462F" strokeWidth={6} fill="none" strokeLinecap="round">
      <path d="M40,176 L380,168" />
      <path d="M110,176 L110,92 M118,176 L118,92 M300,170 L300,86 M308,170 L308,86" />
      <path d="M40,150 Q114,92 150,176 M150,176 Q210,112 270,172 M270,172 Q304,86 380,140" strokeWidth={3} />
    </g>
    <Ink d="M40,176 L380,168" w={1.4} o={0.5} />
    <Cypress x={56} y={312} s={1.2} seed={74} />
  </>),
  'lands-1': () => (<>
    <Sky id="s8" top="#F2A97E" bottom="#FBD9B5" />
    <circle cx={300} cy={120} r={30} fill="#FFE2A0" />
    <Fill d={wob([[0, 150], [400, 140], [400, 300], [0, 300]], 81, 1, true)} fill="#56739B" ink={false} />
    <Fill d={wob([[0, 200], [160, 186], [400, 196], [400, 300], [0, 300]], 82, 1.6, true)} fill="#8E8460" />
    {[60, 46, 32, 18].map((r, i) => <ellipse key={i} cx={170} cy={248} rx={r * 1.8} ry={r * 0.55} fill="none" stroke="#E8DFC8" strokeWidth={5} strokeDasharray={i % 2 ? '10 6' : '18 5'} />)}
    <Ink d={wob([[60, 248], [100, 230], [170, 222], [240, 230], [280, 248]], 83, 1)} w={1.2} o={0.35} />
  </>),
  'lands-2': () => (<>
    <Sky id="s9" top="#BFD8EC" bottom="#EAF0F2" />
    <Fill d={wob([[0, 160], [400, 150], [400, 300], [0, 300]], 91, 1, true)} fill="#3F6F9C" ink={false} />
    <Waves y={176} n={3} seed={92} />
    <Fill d={wob([[0, 210], [140, 200], [280, 214], [400, 204], [400, 300], [0, 300]], 93, 1.6, true)} fill="#B9A274" />
    <path d={wob([[0, 268], [120, 252], [250, 262], [400, 246]], 94, 1)} stroke="#E4D5AE" strokeWidth={10} fill="none" strokeLinecap="round" />
    <Cypress x={90} y={250} s={1.1} seed={95} /><Cypress x={250} y={236} s={0.8} seed={96} /><Cypress x={350} y={258} s={1} seed={97} />
  </>),
};

export function Photo({ trail, i = 0, style }: { trail: TrailId; i?: number; style?: React.CSSProperties }) {
  const Scene = scenes[`${trail}-${i % 3}`];
  return (
    <svg viewBox="0 0 400 300" preserveAspectRatio="xMidYMid slice" role="img" aria-hidden="true" style={{ display: 'block', width: '100%', height: '100%', ...style }}>
      <Scene />
    </svg>
  );
}
