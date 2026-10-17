/** Native WidgetSnapshot wire boundary. No fixture or runtime dependency. */
export interface WidgetSnapshot {
  id: string; appId: string; template: 'stat' | 'progress' | 'countdown' | 'list' | 'caption' | 'ticket' | 'photo';
  title: string; accentHex: string; symbol: string; deepLink: string;
  eyebrow?: string; value?: string; detail?: string; targetMs?: number; sensitive?: boolean;
  bars?: { label: string; value: number }[]; rows?: { text: string; done: boolean }[];
  actions?: { tool: string; label: string }[]; codes?: string[]; places?: string[]; times?: string[];
  art?: string; photo?: string;
}
const templates = new Set(['stat','progress','countdown','list','caption','ticket','photo']);
const photos = new Set(['hike-tomales','hike-steep','hike-lands']);
const arts = new Set(['pet','pet.asleep','pet.gone','globe','pot','ballot','notepad','trip','hike']);
const prefix = (s: string, n: number) => Array.from(s).slice(0,n).join('');
const record = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
/** Mirrors Swift Codable + validated(): reject malformed fields, sanitize permitted text. */
export function parseWidgetSnapshot(input: unknown): WidgetSnapshot | null {
  if (!record(input)) return null;
  const s = input;
  for (const key of ['id','appId','template','title','accentHex','symbol','deepLink']) if (typeof s[key] !== 'string') return null;
  if (!templates.has(s.template as string) || !/^#[0-9a-f]{6}$/i.test(s.accentHex as string) || !/^[a-z0-9.]{1,40}$/.test(s.symbol as string) || s.deepLink !== `zoen://app/${s.id}`) return null;
  const clean = (v: string) => v.includes('://') ? undefined : prefix(v,48);
  const title = clean(s.title as string); if (!title) return null;
  const result: WidgetSnapshot = {id:s.id as string,appId:s.appId as string,template:s.template as WidgetSnapshot['template'],title,accentHex:s.accentHex as string,symbol:s.symbol as string,deepLink:s.deepLink as string};
  for (const key of ['eyebrow','value','detail'] as const) { const v=s[key]; if(v == null) continue; if(typeof v !== 'string') return null; const c=clean(v); if(c !== undefined) result[key]=c; }
  for (const key of ['art','photo'] as const) { const v=s[key]; if(v == null) continue; if(typeof v !== 'string') return null; if((key==='art'?arts:photos).has(v)) result[key]=v; }
  if(s.targetMs != null) { if(typeof s.targetMs !== 'number' || !Number.isSafeInteger(s.targetMs)) return null; result.targetMs=s.targetMs; }
  if(s.sensitive != null) { if(typeof s.sensitive !== 'boolean') return null; result.sensitive=s.sensitive; }
  for(const key of ['codes','places','times'] as const) { const values=s[key]; if(values==null) continue; if(!Array.isArray(values) || !values.every(v=>typeof v==='string')) return null; result[key]=values.slice(0,2).flatMap((v:string)=> { const c=key==='places'?clean(v):v; return c===undefined?[]:[prefix(c,key==='codes'?5:key==='places'?20:8)]; }); }
  if(s.bars!=null) { if(!Array.isArray(s.bars) || !s.bars.every(v=>record(v)&&typeof v.label==='string'&&typeof v.value==='number'&&Number.isFinite(v.value))) return null; result.bars=s.bars.slice(0,3).map(v=>({label:prefix(v.label,16),value:Math.min(1,Math.max(0,v.value))})); }
  if(s.rows!=null) { if(!Array.isArray(s.rows)||!s.rows.every(v=>record(v)&&typeof v.text==='string'&&typeof v.done==='boolean')) return null; result.rows=s.rows.slice(0,4).flatMap(v=>{ const text=clean(v.text); return text===undefined?[]:[{text,done:v.done}]; }); }
  if(s.actions!=null) { if(!Array.isArray(s.actions)||!s.actions.every(v=>record(v)&&typeof v.tool==='string'&&typeof v.label==='string')) return null; result.actions=s.actions.slice(0,2).filter(v=>/^[a-z_]{1,32}$/.test(v.tool)).map(v=>({tool:v.tool,label:prefix(v.label,14)})); }
  if(result.template==='photo'&&!result.photo || result.template==='ticket'&&(result.codes?.length??0)<2) return null;
  return result;
}
export function snapshotRemaining(targetMs: number | undefined, now = Date.now()) {
  const minutes=Math.floor(Math.max(0,(targetMs??0)-now)/60000);
  return {days:Math.floor(minutes/1440),hours:Math.floor(minutes%1440/60),minutes:minutes%60};
}
