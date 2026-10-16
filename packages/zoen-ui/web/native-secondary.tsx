import React, { useState } from 'react';
import './native-secondary.css';
import { NativeAvatar, NativeWelcomeMascot } from './native-art';

export type NativeSettingsPerson = { name: string; handle: string; initials: string; tintHex: string };
const sections = [
  ['Account', ['Profile', 'Security', 'Devices', 'Backup and recovery', 'Your data']],
  ['Preferences', ['Language', 'Theme', 'Accessibility', 'Disappearing messages']],
  ['Zoen', ['Credits and plans', 'Invites', 'Wallet', 'Discover', 'Moderation', 'Tasks']],
  ['Help', ['Feedback', 'Terms and privacy', 'About']],
] as const;

export type NativeSettingsProps = {
  me: NativeSettingsPerson;
  onClose?: () => void;
  onOpen?: (label: string) => void;
};

/** Navigation belongs to the host; the same component is used in parity captures. */
export function NativeSettings({ me, onClose, onOpen }: NativeSettingsProps) {
  const [query, setQuery] = useState('');
  const matches = (label: string) => label.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
  const visibleSections = sections.map(([heading, rows]) => ({ heading, rows: rows.filter(label => matches(heading) || matches(label)) })).filter(({ rows }) => rows.length > 0);
  const showProfile = matches(`Profile ${me.name} ${me.handle}`);
  return <section className="native-settings" aria-label="Settings">
    <header className="ns-top"><h1>Settings</h1><button type="button" className="ns-close" aria-label="Close settings" disabled={!onClose} onClick={onClose}>×</button></header>
    <div className="ns-scroll"><div className="ns-content">
      <label className="ns-search"><span aria-hidden="true">⌕</span><input type="search" aria-label="Search settings" placeholder="Search settings" value={query} onChange={event => setQuery(event.target.value)}/></label>
      {showProfile && <button type="button" className="ns-profile" disabled={!onOpen} onClick={() => onOpen?.('Profile')}><NativeAvatar identity={{ ...me, kind: 'person' }} size={52} className="ns-avatar"/><span><strong>{me.name}</strong><small>@{me.handle}</small></span><span className="ns-chevron" aria-hidden="true">›</span></button>}
      {visibleSections.map(({ heading, rows }) => <section className="ns-group" key={heading}><h2>{heading}</h2><div className="ns-card">{rows.map(label => <button type="button" className="ns-row" key={label} disabled={!onOpen} onClick={() => onOpen?.(label)}><SettingsGlyph label={label}/><span>{label}</span><span className="ns-chevron" aria-hidden="true">›</span></button>)}</div></section>)}
      {!showProfile && visibleSections.length === 0 && <p role="status">No settings found.</p>}
    </div></div>
  </section>;
}

function SettingsGlyph({ label }: { label: string }) {
  // Outline approximations, not substitutes claimed to be SF Symbols.
  const paths: Record<string, string> = {
    Profile: 'M8 8a4 4 0 1 0 8 0 4 4 0 1 0-8 0M4 21v-2a8 8 0 0 1 16 0v2',
    Security: 'M6 10h12v11H6zM8 10V6a4 4 0 0 1 8 0v4',
    Devices: 'M2 4h15v12H2zM0 19h17M18 10h5v12h-5z',
    'Backup and recovery': 'M6 19a5 5 0 0 1-1-10 7 7 0 0 1 13-2 6 6 0 0 1 0 12z',
    'Your data': 'M3 4h18v16H3zM3 15h18M16 18h2',
    Theme: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20M12 2v20',
    'Disappearing messages': 'M9 1h6M12 5a8 8 0 1 0 0 16 8 8 0 1 0 0-16M12 8v6l3 2',
    About: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20M12 10v7M12 6v1',
    'Terms and privacy': 'M5 2h10l4 4v16H5zM15 2v5h4M8 11h8M8 15h8M8 19h5',
    Feedback: 'M3 3h18v14H9l-6 4z',
    Wallet: 'M2 5h20v15H2zM2 9h20M5 16h5',
    Invites: 'M2 8h20v5H2zM4 13v9h16v-9M12 8v14M12 8C0 8 5-3 12 8 19-3 24 8 12 8',
    Moderation: 'M5 23V2l14 1v11L5 13',
    Discover: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20M16 8l-3 6-5 2 3-6z',
    Tasks: 'M3 5h18v17H3zM7 2v6M17 2v6M3 10h18M12 13v4l3 1',
    Accessibility: 'M10 4a2 2 0 1 0 4 0 2 2 0 1 0-4 0M3 8l9 2 9-2M12 10v5M12 15l-5 7M12 15l5 7',
    Language: 'M2 3h20v14H10l-5 4v-4H2zM6 7h6M9 6v6M6 12l6-5M15 12l2-5 2 5M16 10h2',
    'Credits and plans': 'M12 1l3 8 8 3-8 3-3 8-3-8-8-3 8-3z',
  };
  return <svg className="ns-glyph" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[label]}/></svg>;
}

export type NativeOnboardingProps = {
  /** Exact native wave poster URL supplied after the export is available. */
  mascotSrc?: string;
  onContinue?: () => void;
  onPasteInvite?: (value: string) => void;
};

export function NativeOnboarding({ onContinue, onPasteInvite, mascotSrc }: NativeOnboardingProps = {}) {
  const [invite, setInvite] = useState('');
  const [showInvite, setShowInvite] = useState(false);
  return <section className="native-onboarding" aria-label="Welcome">
    <header className="no-header"><div className="no-progress" aria-label="Step 1 of 8">{Array.from({ length: 8 }, (_, i) => <span key={i} className={i === 0 ? 'active' : ''}/>)}</div></header>
    <div className="no-content"><NativeWelcomeMascot src={mascotSrc}/><div className="no-copy"><h1>Hi, I’m Zoen.</h1><p>I’m the app and the agent inside it. I turn chats into plans you control.</p></div><div className="no-invite">Got an invite? <button type="button" disabled={!onPasteInvite} onClick={() => setShowInvite(value => !value)} aria-expanded={showInvite}>Paste</button></div>{showInvite && <form className="no-invite-form" onSubmit={event => { event.preventDefault(); if (invite.trim()) onPasteInvite?.(invite.trim()); }}><label htmlFor="native-invite">Invite link</label><input id="native-invite" type="url" placeholder="Paste your invite link" value={invite} onChange={event => setInvite(event.target.value)} required/><button type="submit" disabled={!onPasteInvite || !invite.trim()}>Use invite</button></form>}</div>
    <footer className="no-footer"><button type="button" className="no-continue" disabled={!onContinue} onClick={onContinue}>Hi, Zoen!</button></footer>
  </section>;
}
