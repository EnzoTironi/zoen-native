import { useEffect, useState, type CSSProperties } from 'react';
import { snapshotRemaining, type WidgetSnapshot } from './widget-snapshot.ts';
import './native-widget.css';
import { displayNow, isDisplayClockFixed } from './display-clock.ts';
export type { WidgetSnapshot } from './widget-snapshot.ts';
export interface NativeHomeStripProps { snapshots: readonly WidgetSnapshot[]; onOpen?: (snapshot: WidgetSnapshot) => void; notificationCount?: number | null; onNotifications?: () => void }
/** Read-only native small-family cards. Opening is delegated to the production host. */
export function NativeHomeStrip({snapshots,onOpen,notificationCount,onNotifications}: NativeHomeStripProps) {
  return <section className="nw-strip" aria-label="Home">{onNotifications && <button className="nw-open nw-notifications" onClick={onNotifications} aria-label="Notifications"><article className="nw-card nw-stat"><div className="nw-content"><strong className="nw-title">Notifications</strong><div className="nw-spacer"/><b className="nw-value">{notificationCount ?? "—"}</b><small className="nw-detail">{notificationCount === null ? "Unavailable on this client" : notificationCount ? "Pending requests" : "View activity"}</small></div></article></button>}{snapshots.length ? snapshots.map(s => onOpen ? <button type="button" className="nw-open" key={s.id} onClick={()=>onOpen(s)}><NativeSnapshotCard snapshot={s}/></button> : <NativeSnapshotCard key={s.id} snapshot={s}/>) : <div className="nw-empty"><img src="assets/native/wave.png" alt=""/><span>Pin a mini-app here</span></div>}</section>;
}
/** SnapshotCard.small geometry: 170 square, continuous 28 corner, 14 content inset. */
export function NativeSnapshotCard({snapshot:s}: {snapshot: WidgetSnapshot}) {
  const [now,setNow]=useState(displayNow);
  useEffect(()=> { if(isDisplayClockFixed() || s.template!=='countdown' && !(s.template==='photo' && s.targetMs!=null)) return; const timer=setInterval(()=>setNow(displayNow()),60000); return ()=>clearInterval(timer); },[s.template,s.targetMs]);
  const r=snapshotRemaining(s.targetMs,now);
  const dark=s.template==='photo'||['globe','trip','pet.asleep'].includes(s.art??'');
  const title=<strong className="nw-title">{s.title}</strong>;
  const countdown=r.days>0?<><b className="nw-days">{r.days}</b><span className="nw-to-go">{r.days===1?'day to go':'days to go'}</span></>:<b className="nw-hours">{r.hours}h {r.minutes}m</b>;
  const label=[s.title,s.eyebrow,s.value,s.detail,...(s.bars??[]).map(b=>`${b.label} ${Math.trunc(b.value*100)}%`),...(s.rows??[]).map(row=>`${row.text}${row.done?', done':''}`),...(s.template==='countdown'||s.template==='photo'&&s.targetMs!=null?[`${r.days} days, ${r.hours} hours and ${r.minutes} minutes to go`]:[]),...(s.template==='ticket'?[`from ${s.places?.[0]??''} at ${s.times?.[0]??''} to ${s.places?.[1]??''} at ${s.times?.[1]??''}`]:[])].filter(Boolean).join(', ');
  return <article className={`nw-card nw-${s.template}${dark?' nw-dark':''}`} data-snapshot-id={s.id} data-art={s.art} data-art-unavailable={s.art ? 'true' : undefined} style={{'--nw-accent':s.accentHex} as CSSProperties} aria-label={label}>
    {s.template==='photo' && s.photo && <><img className={`nw-photo-image${s.targetMs!=null?' nw-photo-zoom':''}`} src={`assets/native/${s.photo}.jpg`} alt=""/><span className={`nw-photo-shade${s.targetMs!=null?' nw-photo-countdown':''}`}/></>}
    <div className="nw-content" aria-hidden="true">
      {s.template==='stat' && <>{title}<div className="nw-spacer"/>{s.value!=null&&<b className="nw-value">{s.value}</b>}{s.detail!=null&&<small className="nw-detail">{s.detail}</small>}</>}
      {s.template==='progress' && <>{title}<div className="nw-spacer"/><div className={`nw-bars${s.art?.startsWith('pet')?' nw-bars-compact':''}`}>{(s.bars??[]).map((b,i)=><div className="nw-bar-row" key={i}>{!s.art?.startsWith('pet')&&<span>{b.label}</span>}<span className="nw-gauge"><i style={{width:`max(6px, ${b.value*100}%)`,background:b.value<.25?'#E5484D':s.accentHex}}/></span></div>)}</div></>}
      {s.template==='countdown' && <>{title}<div className="nw-spacer"/>{countdown}</>}
      {s.template==='list' && <><div className="nw-list-heading">{title}{s.value!=null&&<b>{s.value}</b>}</div>{(s.rows??[]).map((row,i)=><div className={`nw-list-row${row.done?' nw-done':''}`} key={i}><svg viewBox="0 0 16 16" fill={row.done?'var(--nw-accent)':'none'} stroke={row.done?'var(--nw-accent)':'currentColor'}><circle cx="8" cy="8" r="6.2"/>{row.done&&<path d="m4.6 8 2.2 2.2 4.6-4.6" fill="none" stroke="white" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>}</svg><span>{row.text}</span></div>)}</>}
      {s.template==='caption' && <>{title}{s.detail!=null&&<p className="nw-caption-detail">{s.detail}</p>}</>}
      {s.template==='ticket' && <><span className="nw-ticket-eyebrow">{s.eyebrow??s.title}</span><div className="nw-spacer"/><div className="nw-ticket-codes"><b>{s.codes?.[0]}</b><b>{s.codes?.[1]}</b></div><div className="nw-ticket-places"><span>{s.places?.[0]}</span><span>{s.places?.[1]}</span></div><div className="nw-spacer"/><div className="nw-route"><span/><span/></div><div className="nw-spacer"/><div className="nw-ticket-times"><div><small>Departure</small><b>{s.times?.[0]}</b></div><div><small>Arrival</small><b>{s.times?.[1]}</b></div></div></>}
      {s.template==='photo' && (s.targetMs!=null?<div className="nw-glass-countdown"><strong>{s.title}</strong><div className="nw-cells">{([[r.days,'DAYS'],[r.hours,'HRS'],[r.minutes,'MIN']] as const).map(([n,label])=><div key={label}><b>{String(Math.min(n,99)).padStart(2,'0')}</b><span>{label}</span></div>)}</div></div>:<div className="nw-photo-caption"><strong>{s.title}</strong>{s.eyebrow!=null&&<span>{s.eyebrow}</span>}</div>)}
    </div>
  </article>;
}
