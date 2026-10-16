import './hackathon.css';

/** Render only after an authoritative action settles; revision zero is silent. */
export function LivingConfirmation({ revision, label, positive = true }: { revision: number; label: string; positive?: boolean }) {
  return revision ? <p key={`${revision}-${positive}`} className="zh-feedback" role={positive ? 'status' : 'alert'}><svg aria-hidden="true" viewBox="0 0 24 24" width="24" height="24"><circle pathLength="1" cx="12" cy="12" r="10" /><path pathLength="1" d={positive ? 'm7 12 3 3 7-7' : 'm8 8 8 8 m0-8-8 8'} /></svg>{label}</p> : null;
}
