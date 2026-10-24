import { useEffect, useRef, useState } from 'react';
import maplibregl from 'maplibre-gl';
import { zoen } from '@zoen/ui';
import { bounds, type Trail } from './trails';
import { OfflineMap } from './OfflineMap';

const ONLINE_STYLE = 'https://tiles.openfreemap.org/styles/liberty';

/** Critically damped spring as an easing curve (no overshoot on the camera). */
const springEase = (t: number) => { const k = 7; return (1 - (1 + k * t) * Math.exp(-k * t)) / (1 - (1 + k) * Math.exp(-k)); };

export function MapView({ trail }: { trail: Trail }) {
  const [online, setOnline] = useState(false);
  useEffect(() => {
    let mounted = true;
    zoen.ready().then(() => {
      if (mounted) setOnline(zoen.capabilities.has('net:tiles.openfreemap.org') || new URLSearchParams(location.search).has('net'));
    });
    return () => { mounted = false; };
  }, []);
  return online ? <OnlineMapView trail={trail} /> : <OfflineMap trail={trail} />;
}

function OnlineMapView({ trail }: { trail: Trail }) {
  const el = useRef<HTMLDivElement>(null);
  const map = useRef<maplibregl.Map | null>(null);
  const marker = useRef<maplibregl.Marker | null>(null);
  const ready = useRef(false);
  const latestTrail = useRef(trail);
  const [failed, setFailed] = useState(false);

  const show = (t: Trail, animate: boolean) => {
    const m = map.current; if (!m || !ready.current) return;
    (m.getSource('route') as maplibregl.GeoJSONSource).setData({ type: 'Feature', properties: {}, geometry: { type: 'LineString', coordinates: t.line } });
    const mid = t.line[Math.floor(t.line.length / 2)];
    marker.current!.setLngLat(mid);
    (marker.current!.getElement().firstChild as HTMLElement).textContent = t.name;
    m.fitBounds(bounds(t.line), { padding: { top: 150, bottom: 300, left: 50, right: 50 }, duration: animate ? 1200 : 0, easing: springEase, maxZoom: 14.5 });
  };

  useEffect(() => {
    if (failed || !el.current) return;
    let m: maplibregl.Map;
    try {
      m = new maplibregl.Map({ container: el.current, style: ONLINE_STYLE, center: trail.trailhead, zoom: 11, attributionControl: { compact: true }, pitchWithRotate: false, dragRotate: false });
    } catch { setFailed(true); return; }
    map.current = m;
    m.on('error', () => { if (!ready.current) setFailed(true); });
    const pin = document.createElement('div');
    const label = document.createElement('div'); label.className = 'marker'; pin.appendChild(label);
    marker.current = new maplibregl.Marker({ element: pin, anchor: 'bottom', offset: [0, -6] }).setLngLat(trail.trailhead).addTo(m);
    m.on('load', () => {
      const current = latestTrail.current;
      m.addSource('route', { type: 'geojson', data: { type: 'Feature', properties: {}, geometry: { type: 'LineString', coordinates: current.line } } });
      m.addLayer({ id: 'route-casing', type: 'line', source: 'route', paint: { 'line-color': '#ffffff', 'line-width': 9, 'line-opacity': 0.95 }, layout: { 'line-cap': 'round', 'line-join': 'round' } });
      m.addLayer({ id: 'route', type: 'line', source: 'route', paint: { 'line-color': '#3D7A28', 'line-width': 4.5 }, layout: { 'line-cap': 'round', 'line-join': 'round' } });
      ready.current = true;
      show(current, false);
    });
    return () => { ready.current = false; map.current = null; marker.current = null; m.remove(); };
  }, [failed]);

  useEffect(() => { latestTrail.current = trail; show(trail, true); }, [trail]);

  return failed ? <OfflineMap trail={trail} /> : <div ref={el} className="map" data-map="online" aria-label={`Map of ${trail.name}`} role="img" />;
}

