import { useState } from 'react';
import { NativeSettings, type NativeSettingsPerson } from './native-secondary.tsx';
import { applyAppearance, type Appearance } from './appearance.ts';
export function BrowserSettings({ me, close, openDevices, forget, failed = false, showAccountProfile = true }: { me: NativeSettingsPerson; close: () => void; openDevices?: () => void; forget?: () => void; failed?: boolean; showAccountProfile?: boolean }) {
  const [page, setPage] = useState<string | null>(null);
  const content = !page ? <NativeSettings me={me} showAccountProfile={showAccountProfile} onClose={close} onOpen={label => { if (label === 'Devices' && openDevices) openDevices(); else setPage(label); }} /> : <section className="native-settings ns-detail-page zw-account" role="dialog" aria-modal="true" aria-label={page}><button className="zw-secondary" onClick={() => setPage(null)}>Back to Settings</button><h1>{page}</h1>
    {page === 'Theme' ? <label>Appearance <select aria-label="Appearance" defaultValue={localStorage.getItem('zoen.appearance') ?? 'system'} onChange={event => { localStorage.setItem('zoen.appearance', event.target.value); applyAppearance(event.target.value as Appearance); }}><option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option></select></label> : page === 'Your data' ? <><p>This browser stores your encrypted conversations and device key locally.</p><button className="zw-secondary" disabled={!forget} onClick={forget}>Sign out and erase this browser</button></> : page === 'About' ? <><p>Zoen — your conversations, end-to-end encrypted.</p>{failed ? <button className="zw-secondary" onClick={() => location.reload()}>Reload browser</button> : null}</> : <p>This setting is managed in the Zoen app on your phone. It is not available in this linked browser yet.</p>}
  </section>;
  return <div className="ns-overlay" onKeyDown={event => { if (event.key === 'Escape') close(); }}>{content}</div>;
}
