import { useRef, useState, type KeyboardEvent } from 'react';
import { ShellIcon, ZoenShell, type ShellTheme } from '@zoen/ui/shell';

type LastStop = 'a' | 'b';

export function FocusFixtureFooter({
  lastStop,
  onLastStopChange,
}: {
  readonly lastStop: LastStop;
  readonly onLastStopChange: (stop: LastStop) => void;
}) {
  const aRef = useRef<HTMLButtonElement>(null);
  const bRef = useRef<HTMLButtonElement>(null);

  function handleRovingKey(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    let next: LastStop;
    switch (event.key) {
      case 'ArrowLeft':
      case 'ArrowUp':
      case 'Home':
        next = 'a';
        break;
      case 'ArrowRight':
      case 'ArrowDown':
      case 'End':
        next = 'b';
        break;
      default:
        return;
    }
    event.preventDefault();
    onLastStopChange(next);
    (next === 'a' ? aRef : bRef).current?.focus();
  }

  return (
    <div className="focus-fixture-footer">
      <details open>
        <summary>Host-slot disclosure</summary>
        <p>This native summary belongs in the drawer's Tab order.</p>
      </details>
      <div role="toolbar" aria-label="Roving footer controls">
        <button
          ref={aRef}
          type="button"
          tabIndex={lastStop === 'a' ? 0 : -1}
          aria-pressed={lastStop === 'a'}
          onClick={(event) => {
            onLastStopChange('a');
            event.currentTarget.focus();
          }}
          onKeyDown={handleRovingKey}
        >
          Roving stop A
        </button>
        <button
          ref={bRef}
          type="button"
          tabIndex={lastStop === 'b' ? 0 : -1}
          aria-pressed={lastStop === 'b'}
          onClick={(event) => {
            onLastStopChange('b');
            event.currentTarget.focus();
          }}
          onKeyDown={handleRovingKey}
        >
          Roving stop B
        </button>
      </div>
      <p role="status" aria-live="polite">
        Last Tab stop: Roving stop {lastStop.toUpperCase()}
      </p>
      <button type="button" tabIndex={-1} className="focus-fixture-negative">
        Negative tab button (never in Tab order)
      </button>
    </div>
  );
}

export function DrawerFocusFixture() {
  const [lastStop, setLastStop] = useState<LastStop>('a');
  const [query, setQuery] = useState('');
  const [theme, setTheme] = useState<ShellTheme>('system');
  return (
    <ZoenShell
      navigation={[{ id: 'chats', label: 'Chats', icon: <ShellIcon name="chats" /> }]}
      activeNavigationId="chats"
      onNavigate={() => {}}
      chats={[{ id: 'fixture', title: 'Fixture conversation', preview: 'Keyboard checks only' }]}
      activeChatId="fixture"
      onSelectChat={() => {}}
      searchQuery={query}
      onSearchChange={setQuery}
      theme={theme}
      onThemeChange={setTheme}
      defaultCollapsed
      sidebarFooter={<FocusFixtureFooter lastStop={lastStop} onLastStopChange={setLastStop} />}
    >
      <section className="focus-fixture-instructions">
        <h1>Drawer host-slot focus fixture</h1>
        <p>
          This separate verification page uses the exported ZoenShell and its sidebar footer slot.
        </p>
        <ol>
          <li>Use a viewport narrower than 760 px. Open Show chats in the rail.</li>
          <li>Focus starts at Close chats. Shift-Tab should reach Roving stop A.</li>
          <li>
            Tab should wrap to Close chats. Tab forward through search, the chat row, and Host-slot
            disclosure.
          </li>
          <li>
            From Roving stop A, press Right Arrow or click Roving stop B. B becomes the last Tab
            stop.
          </li>
          <li>
            Tab from B wraps to Close chats; Shift-Tab returns to B. Both negative buttons are
            skipped.
          </li>
          <li>Escape closes the drawer and returns focus to Show chats.</li>
        </ol>
      </section>
    </ZoenShell>
  );
}
