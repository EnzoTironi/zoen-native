import { NativeAvatar } from './native-art';
import type { NativeAvatarIdentity } from './native-art';
import './native-hike-card.css';

export type NativeHikeApp = { appId: string; name: string; view: Record<string, unknown> };
const record = (v: unknown): Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v) ? v as Record<string, unknown> : {};
const string = (v: unknown): string | undefined => typeof v === 'string' ? v : undefined;
const rows = (v: unknown): Record<string, unknown>[] => Array.isArray(v) ? v.map(record) : [];

/** HikeInfo from the projected app state, never reconstructed from a widget snapshot. */
export function parseHikeInfo(app: NativeHikeApp, language = typeof document === 'undefined' ? 'en' : document.documentElement.lang) {
  const pt = language.toLowerCase().startsWith('pt');
  const labels: Record<string, string> = pt ? { tomales: 'Praia do Sono', steep: 'Pedra Grande', lands: 'Pico do Jaraguá' } : { tomales: 'Tomales Point', steep: 'Steep Ravine', lands: 'Lands End' };
  const trails = rows(app.view.trails);
  const votes = trails.map(row => ({ trail: string(row.id) ?? '', names: Array.isArray(row.votes) && row.votes.every(n => typeof n === 'string') ? row.votes as string[] : [] }));
  for (const row of trails) {
    const id = string(row.id), name = string(row.name);
    if (id !== undefined) labels[id] = name?.trim() ? name : labels[id] ?? id;
  }
  const displayName = (id: string) => labels[id] ?? id;
  const title = string(app.view.title) ?? app.name;
  const day = string(app.view.day) ?? '';
  const decided = string(app.view.decided);
  const total = votes.reduce((n, row) => n + row.names.length, 0);
  // Swift max(by:) retains the first equal maximum.
  const leading = votes.reduce<typeof votes[number] | undefined>((best, row) => !best || row.names.length > best.names.length ? row : best, undefined);
  const leader = leading?.names.length ? leading.trail : undefined;
  const photoId = decided ?? leader ?? 'tomales';
  const photo = ['tomales', 'steep', 'lands'].includes(photoId) ? `./assets/native/hike-${photoId}.jpg` : undefined;
  const first = rows(app.view.itinerary)[0];
  const firstStep = first ? `${string(first.time) ?? ''} ${string(first.text) ?? ''}` : undefined;
  const status = decided !== undefined ? `${displayName(decided)} • ${day}` : total === 0 ? pt ? '3 trilhas • Mapas e fotos' : '3 trails • Maps & photos' : pt ? `${total} votos • ${displayName(leader ?? 'tomales')} na frente` : `${total} votes • ${displayName(leader ?? 'tomales')} leads`;
  return { title, day, decided, votes, total, voters: votes.flatMap(row => row.names), leader, photo, firstStep, status, displayName };
}

export function NativeHikeChatCard({ app, onOpen, people = [] }: { app: NativeHikeApp; onOpen?: (appId: string) => void; people?: NativeAvatarIdentity[] }) {
  const hike = parseHikeInfo(app);
  // Limit names before lookup, matching native's Set.sorted.prefix then if-persona.
  const voters = [...new Set(hike.voters)].sort().slice(0, 3).flatMap(name => { const person = people.find(p => p.name === name && p.kind === 'person'); return person ? [person] : []; });
  return <button type="button" className="native-hike-card" onClick={() => onOpen?.(app.appId)} aria-label={`${hike.title}, ${hike.status}`}>
    {hike.photo ? <img className="native-hike-photo" src={hike.photo} alt="" width="58" height="58"/> : <span className="native-hike-photo" aria-hidden="true"/>}
    <span className="native-hike-copy"><span className="native-hike-title">{hike.title}</span><span className="native-hike-status">{hike.status}</span></span>
    <span className="native-hike-spacer"/>
    {voters.length > 0 && <span className="native-hike-voters">{voters.map(person => <NativeAvatar key={person.name} identity={person} size={20}/>)}</span>}
    <svg className="native-hike-chevron" width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m9 5 7 7-7 7" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/></svg>
  </button>;
}
