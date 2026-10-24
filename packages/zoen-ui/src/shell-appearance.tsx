import { useRef, useState } from 'react';
import { ShellIcon, type ShellIconName } from './shell-icons';

export type ShellTheme = 'light' | 'dark' | 'system';

interface ThemeControlProps {
  readonly theme: ShellTheme;
  readonly onThemeChange: (theme: ShellTheme) => void;
}

const themeOptions = {
  dark: { label: 'Dark appearance', icon: 'moon' },
  system: { label: 'System appearance', icon: 'system' },
  light: { label: 'Light appearance', icon: 'sun' },
} satisfies Record<ShellTheme, { label: string; icon: ShellIconName }>;
type ThemePosition = 'left' | 'center' | 'right';
const themeNeighbors: Record<ShellTheme, Record<ThemePosition, ShellTheme>> = {
  dark: { left: 'light', center: 'dark', right: 'system' },
  system: { left: 'dark', center: 'system', right: 'light' },
  light: { left: 'system', center: 'light', right: 'dark' },
};
const themePositions: readonly ThemePosition[] = ['left', 'center', 'right'];
const themeModes: readonly ShellTheme[] = ['dark', 'system', 'light'];
type ThemeMotion =
  | { kind: 'still'; theme: ShellTheme }
  | { kind: 'rolling'; theme: ShellTheme; direction: 'left' | 'right' };

export function ThemeControl({ theme, onThemeChange }: ThemeControlProps) {
  const centerRef = useRef<HTMLButtonElement>(null);
  const [motion, setMotion] = useState<ThemeMotion>({ kind: 'still', theme });
  if (motion.theme !== theme) {
    setMotion({
      kind: 'rolling',
      theme,
      direction: themeNeighbors[motion.theme].right === theme ? 'left' : 'right',
    });
  }
  const neighbors = themeNeighbors[theme];
  function select(value: ShellTheme) {
    if (value !== theme) onThemeChange(value);
    centerRef.current?.focus();
  }
  return (
    <div
      className="zs-appearance"
      role="group"
      aria-label="Appearance"
      onKeyDown={(event) => {
        if (event.altKey || event.ctrlKey || event.metaKey) return;
        let value: ShellTheme;
        switch (event.key) {
          case 'ArrowLeft':
          case 'ArrowUp':
            value = neighbors.left;
            break;
          case 'ArrowRight':
          case 'ArrowDown':
            value = neighbors.right;
            break;
          case 'Home':
            value = 'dark';
            break;
          case 'End':
            value = 'light';
            break;
          default:
            return;
        }
        event.preventDefault();
        select(value);
      }}
    >
      {themePositions.map((position) => {
        const value = neighbors[position];
        const option = themeOptions[value];
        return (
          <button
            key={position}
            ref={position === 'center' ? centerRef : undefined}
            type="button"
            className="zs-theme-target"
            data-position={position}
            aria-label={option.label}
            title={option.label}
            aria-pressed={value === theme}
            tabIndex={position === 'center' ? 0 : -1}
            onClick={() => select(value)}
          >
            <span className="zs-sr-only">{option.label}</span>
          </button>
        );
      })}
      <div
        key={theme}
        className="zs-theme-window"
        data-motion={motion.kind === 'rolling' ? motion.direction : 'still'}
        aria-hidden="true"
      >
        {themeModes.map((value) => {
          const position =
            neighbors.left === value ? 'left' : neighbors.right === value ? 'right' : 'center';
          return (
            <span key={value} className="zs-theme-icon" data-position={position}>
              <ShellIcon name={themeOptions[value].icon} />
            </span>
          );
        })}
        {motion.kind === 'rolling' && (
          <span
            className="zs-theme-icon"
            data-position={motion.direction === 'left' ? 'outgoing-left' : 'outgoing-right'}
          >
            <ShellIcon
              name={
                themeOptions[motion.direction === 'left' ? neighbors.right : neighbors.left].icon
              }
            />
          </span>
        )}
      </div>
    </div>
  );
}

