import type { ReactNode } from 'react';

export type ShellIconName =
  | 'chats'
  | 'spaces'
  | 'apps'
  | 'store'
  | 'files'
  | 'agents'
  | 'context'
  | 'activity'
  | 'search'
  | 'panel'
  | 'compose'
  | 'close'
  | 'sun'
  | 'moon'
  | 'system';

const iconPaths: Record<ShellIconName, ReactNode> = {
  chats: <path d="M21 11.5a8.5 8.5 0 0 1-8.5 8.5H4l-1 1v-9.5a8.5 8.5 0 0 1 17 0Z" />,
  spaces: (
    <>
      <circle cx="12" cy="7" r="3" />
      <path d="M6 21v-2a6 6 0 0 1 12 0v2M5 5a3 3 0 0 0 0 6M19 5a3 3 0 0 1 0 6M2 19v-1a5 5 0 0 1 3-4m17 5v-1a5 5 0 0 0-3-4" />
    </>
  ),
  apps: (
    <>
      <rect x="3" y="3" width="7" height="7" rx="2" />
      <rect x="14" y="3" width="7" height="7" rx="2" />
      <rect x="3" y="14" width="7" height="7" rx="2" />
      <rect x="14" y="14" width="7" height="7" rx="2" />
    </>
  ),
  store: (
    <>
      <path d="M4 10v11h16V10M3 10l2-7h14l2 7M3 10a3 3 0 0 0 4.5 2.5A3 3 0 0 0 12 12a3 3 0 0 0 4.5.5A3 3 0 0 0 21 10M10 21v-6h4v6" />
    </>
  ),
  files: (
    <path d="M3 7V5a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z" />
  ),
  agents: (
    <>
      <rect x="3" y="5" width="18" height="14" rx="3" />
      <path d="M7 22h10M12 8l1.1 3.9L17 13l-3.9 1.1L12 18l-1.1-3.9L7 13l3.9-1.1L12 8Zm6-6v4m-2-2h4" />
    </>
  ),
  context: (
    <>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="9" r="3" />
      <path d="M5.5 18a7 7 0 0 1 13 0" />
    </>
  ),
  activity: (
    <>
      <path d="M5 17h14l-2-3V9a5 5 0 0 0-10 0v5l-2 3ZM10 21h4M12 2v2" />
    </>
  ),
  search: (
    <>
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="m16 16 4.5 4.5" />
    </>
  ),
  panel: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="3" />
      <path d="M9 4v16m-3-9v2" />
    </>
  ),
  compose: (
    <>
      <path d="M12 4H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h13a2 2 0 0 0 2-2v-7M14 5l5 5m-9 5 2-6 7-7 3 3-7 7-5 3Z" />
    </>
  ),
  close: <path d="m6 6 12 12M18 6 6 18" />,
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5" />
    </>
  ),
  moon: <path d="M20.5 13.2A8.5 8.5 0 0 1 10.8 3.5a8.5 8.5 0 1 0 9.7 9.7Z" />,
  system: (
    <>
      <rect x="3" y="4" width="18" height="13" rx="2" />
      <path d="M12 17v4m-4 0h8" />
      <g style={{ display: 'var(--zs-system-sun-display, block)' }} strokeWidth="1.1">
        <circle cx="12" cy="10.5" r="2.3" />
        <path d="M12 6v1m0 7v1m-4.5-4.5h1m7 0h1M9 7.5l.6.6m4.8 4.8.6.6m-6 0 .6-.6m4.8-4.8.6-.6" />
      </g>
      <path
        style={{ display: 'var(--zs-system-moon-display, none)' }}
        d="M15.5 11.1a3.4 3.4 0 0 1-4.1-4.1 3.4 3.4 0 1 0 4.1 4.1Z"
        strokeWidth="1.1"
      />
    </>
  ),
};

export function ShellIcon({ name }: { name: ShellIconName }) {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {iconPaths[name]}
    </svg>
  );
}

