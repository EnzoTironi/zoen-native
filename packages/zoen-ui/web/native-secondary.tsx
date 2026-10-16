import React from 'react';
import './native-secondary.css';

export type NativeSettingsPerson = { name: string; handle: string; initials: string; tintHex: string };
const sections = [
  ['Account', ['Profile', 'Security', 'Devices', 'Backup and recovery', 'Your data']],
  ['Preferences', ['Language', 'Theme', 'Accessibility', 'Disappearing messages']],
  ['Zoen', ['Credits and plans', 'Invites', 'Wallet', 'Discover', 'Moderation', 'Tasks']],
  ['Help', ['Feedback', 'Terms and privacy', 'About']],
] as const;

// Read-only visual fixtures: no account, navigation, clipboard or permission side effects.
export function NativeSettings({ me }: { me: NativeSettingsPerson }) {
  return <section className="native-settings" aria-label="Settings visual fixture">
    <header className="ns-top"><h1>Settings</h1><span className="ns-close" aria-hidden="true">×</span></header>
    <div className="ns-scroll"><div className="ns-content">
      <div className="ns-search"><span aria-hidden="true">⌕</span><span>Search settings</span></div>
      <div className="ns-profile"><span className="ns-avatar" style={{ backgroundColor: me.tintHex }}>{me.initials}</span><div><strong>{me.name}</strong><small>@{me.handle}</small></div><span className="ns-chevron" aria-hidden="true">›</span></div>
      {sections.map(([heading, rows]) => <section className="ns-group" key={heading}><h2>{heading}</h2><div className="ns-card">{rows.map(label => <div className="ns-row" key={label}><SettingsGlyph label={label}/><span>{label}</span><span className="ns-chevron" aria-hidden="true">›</span></div>)}</div></section>)}
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

/** Static silhouette study of Mascot.swift's green felt rig and wave pose. Not Ink-renderer parity. */
function WelcomeMascot() {
  const hairs = Array.from({ length: 96 }, (_, i) => {
    const a = i * Math.PI * 2 / 96; const x = 112 + Math.cos(a) * 61; const y = 119 + Math.sin(a) * 70;
    return <path key={i} d={`M${x} ${y}q${Math.cos(a) * 6 - 2} ${Math.sin(a) * 6} ${Math.cos(a) * 8} ${Math.sin(a) * 9}`} />;
  });
  return <svg className="no-mascot" viewBox="0 0 230 230" role="img" aria-label="Zoen, a green furry mascot giving a reluctant wave">
    <ellipse cx="115" cy="207" rx="39" ry="5" fill="#2B4019" opacity=".14"/>
    <g fill="#6A9A36" stroke="#2B4019" strokeWidth="1.5" strokeLinecap="round">
      <ellipse cx="91" cy="201" rx="12" ry="8"/><ellipse cx="131" cy="201" rx="12" ry="8"/>
      <path d="M168 119q22-17 16-43q-2-11-9-4q-6-8-10-1q-6-4-9 4l7 26-13 7"/>
      <ellipse cx="112" cy="119" rx="62" ry="71"/><ellipse cx="53" cy="141" rx="10" ry="9"/>
      <path d="M105 50l-6-12m13 10 1-16m7 17 8-12" fill="none" strokeWidth="3"/>
      <g fill="none" strokeWidth="1" opacity=".65">{hairs}</g>
    </g>
    <ellipse cx="100" cy="81" rx="31" ry="19" fill="#A9CC6E" opacity=".28"/>
    <path d="M54 80q58-19 116 0" fill="none" stroke="#E0533E" strokeWidth="14"/>
    <path d="M54 80q58-19 116 0" fill="none" stroke="white" strokeWidth="4"/>
    <g stroke="#2B4019" strokeWidth="2"><ellipse cx="90" cy="111" rx="13" ry="15" fill="#F4F6EC"/><ellipse cx="127" cy="111" rx="13" ry="15" fill="#F4F6EC"/><ellipse cx="94" cy="115" rx="6" ry="8" fill="#56634A"/><ellipse cx="131" cy="115" rx="6" ry="8" fill="#56634A"/><path d="m76 96 26 8m13 0 26-8M100 148q11-5 22 0" fill="none" strokeWidth="3"/></g>
    <g fill="#1B2213"><circle cx="95" cy="116" r="3"/><circle cx="132" cy="116" r="3"/></g>
    <g fill="white"><circle cx="94" cy="113" r="2"/><circle cx="131" cy="113" r="2"/></g>
  </svg>;
}

export function NativeOnboarding() {
  return <section className="native-onboarding" aria-label="Welcome visual fixture">
    <header className="no-header"><div className="no-progress" aria-label="Step 1 of 8">{Array.from({ length: 8 }, (_, i) => <span key={i} className={i === 0 ? 'active' : ''}/>)}</div></header>
    <div className="no-content"><WelcomeMascot/><div className="no-copy"><h1>Hi, I’m Zoen.</h1><p>I’m the app and the agent inside it. I turn chats into plans you control.</p></div><p className="no-invite">Got an invite? <span>Paste</span></p></div>
    <footer className="no-footer"><div className="no-continue">Hi, Zoen!</div></footer>
  </section>;
}
