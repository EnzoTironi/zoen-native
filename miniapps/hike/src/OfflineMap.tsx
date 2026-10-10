import { useEffect, useMemo, useRef, useState } from 'react';
import type { PointerEvent, KeyboardEvent, WheelEvent } from 'react';
import type { Trail } from './trails';
import { fitRoute, gesture, initialCamera, moveCamera, type Point, type Size } from './mapGeometry';
import { L } from './i18n';

/** Local routes need no tiles, workers or WebGL. The map remains usable offline. */
export function OfflineMap({ trail }: { trail: Trail }) {
  const svg = useRef<SVGSVGElement>(null);
  const pointers = useRef(new Map<number, Point>());
  const [size, setSize] = useState<Size>({ width: 360, height: 720 });
  const [camera, setCamera] = useState(initialCamera);
  useEffect(() => {
    const element = svg.current;
    if (!element) return;
    const resize = () => setSize({ width: Math.max(1, element.clientWidth), height: Math.max(1, element.clientHeight) });
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useEffect(() => { setCamera(initialCamera); pointers.current.clear(); }, [trail.id, size.width, size.height]);
  const points = useMemo(() => fitRoute(trail.line, size), [trail.line, size]);
  const path = points.map((p, i) => `${i ? 'L' : 'M'}${p.x},${p.y}`).join(' ');
  const marker = points[Math.floor(points.length / 2)];
  const start = points[0];
  const centre = { x: size.width / 2, y: size.height / 2 };
  const zoom = (factor: number, at = centre) => setCamera(current => moveCamera(current, size, at, at, factor));
  const point = (e: PointerEvent<SVGSVGElement>): Point => {
    const rect = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };
  const down = (e: PointerEvent<SVGSVGElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    pointers.current.set(e.pointerId, point(e));
  };
  const move = (e: PointerEvent<SVGSVGElement>) => {
    if (!pointers.current.has(e.pointerId)) return;
    const before = gesture([...pointers.current.values()]);
    pointers.current.set(e.pointerId, point(e));
    const after = gesture([...pointers.current.values()]);
    if (before && after) setCamera(current => moveCamera(current, size, before.centre, after.centre, after.distance / before.distance));
  };
  const release = (e: PointerEvent<SVGSVGElement>) => { pointers.current.delete(e.pointerId); };
  const wheel = (e: WheelEvent<SVGSVGElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    zoom(Math.exp(-Math.max(-100, Math.min(100, e.deltaY)) * .005), { x: e.clientX - rect.left, y: e.clientY - rect.top });
  };
  const key = (e: KeyboardEvent<SVGSVGElement>) => {
    const offsets: Record<string, Point> = { ArrowLeft: { x: 40, y: 0 }, ArrowRight: { x: -40, y: 0 }, ArrowUp: { x: 0, y: 40 }, ArrowDown: { x: 0, y: -40 } };
    const offset = offsets[e.key];
    if (offset) { e.preventDefault(); setCamera(c => ({ ...c, x: c.x + offset.x, y: c.y + offset.y })); }
    if (e.key === '+' || e.key === '=') { e.preventDefault(); zoom(1.5); }
    if (e.key === '-') { e.preventDefault(); zoom(1 / 1.5); }
    if (e.key === '0') { e.preventDefault(); setCamera(initialCamera); }
  };
  return <div className="map offline-map" data-map="offline">
    <svg ref={svg} viewBox={`0 0 ${size.width} ${size.height}`} role="img" tabIndex={0}
      aria-label={`${L('Offline route')}: ${trail.name}. ${L('Drag to move, pinch to zoom')}`}
      onPointerDown={down} onPointerMove={move} onPointerUp={release} onPointerCancel={release} onLostPointerCapture={release} onWheel={wheel} onKeyDown={key}>
      <g transform={`translate(${camera.x} ${camera.y}) translate(${centre.x} ${centre.y}) scale(${camera.zoom}) translate(${-centre.x} ${-centre.y})`}>
        <path d={path} fill="none" stroke="#fff" strokeWidth={9} strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
        <path d={path} fill="none" stroke="#3D7A28" strokeWidth={4.5} strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
        {start && <circle cx={start.x} cy={start.y} r={6 / camera.zoom} fill="#fff" stroke="#3D7A28" strokeWidth={2} vectorEffect="non-scaling-stroke" />}
        {marker && <g transform={`translate(${marker.x} ${marker.y - 18 / camera.zoom}) scale(${1 / camera.zoom})`}>
          <rect x={-(trail.name.length * 7.5 + 24) / 2} y={-30} width={trail.name.length * 7.5 + 24} height={30} rx={15} fill="#111113" />
          <path d="M-5,0 L0,6 L5,0" fill="#111113" />
          <text y={-10} textAnchor="middle" fill="#fff" fontSize={13} fontWeight={700}>{trail.name}</text>
        </g>}
      </g>
      <text x={16} y={size.height / 2} fill="#5B6478" fontSize={12}>N ↑</text>
    </svg>
    <div className="offline-controls" aria-label={L('Map controls')}>
      <button aria-label={L('Zoom in')} onClick={() => zoom(1.5)}>+</button>
      <button aria-label={L('Zoom out')} onClick={() => zoom(1 / 1.5)}>−</button>
      <button aria-label={L('Fit route')} onClick={() => setCamera(initialCamera)}>↺</button>
    </div>
    <span className="offline-note">{L('Offline route')}</span>
  </div>;
}
