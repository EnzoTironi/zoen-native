import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type MouseEventHandler,
  type ReactNode,
  type Ref,
} from 'react';

export type ShellTheme = 'light' | 'dark' | 'system';
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

export interface ShellNavigationItem<Id extends string = string> {
  readonly id: Id;
  readonly label: string;
  readonly icon: ReactNode;
  readonly badge?: number;
}

export interface ShellChatItem<Id extends string = string> {
  readonly id: Id;
  readonly title: string;
  readonly preview?: string;
  readonly time?: string;
  readonly avatar?: ReactNode;
  readonly unreadCount?: number;
}

export interface ZoenShellProps<
  NavigationId extends string = string,
  ChatId extends string = string,
> {
  readonly navigation: readonly ShellNavigationItem<NavigationId>[];
  readonly activeNavigationId: NavigationId;
  readonly onNavigate: (id: NavigationId) => void;
  readonly searchNavigationId?: NavigationId;
  readonly chats: readonly ShellChatItem<ChatId>[];
  readonly activeChatId?: ChatId;
  readonly onSelectChat: (id: ChatId) => void;
  readonly searchQuery: string;
  readonly onSearchChange: (query: string) => void;
  readonly theme: ShellTheme;
  readonly onThemeChange: (theme: ShellTheme) => void;
  readonly title?: string;
  readonly children: ReactNode;
  readonly sidebarTitle?: string;
  readonly searchPlaceholder?: string;
  readonly searchInputRef?: Ref<HTMLInputElement>;
  readonly globalSearchShortcut?: boolean;
  readonly headerActions?: ReactNode;
  readonly sidebarFilters?: ReactNode;
  readonly sidebarFooter?: ReactNode;
  readonly emptySidebar?: ReactNode;
  readonly onNewChat?: () => void;
  readonly defaultCollapsed?: boolean;
  readonly className?: string;
}

function UnreadBadge({ count }: { count: number | undefined }) {
  if (count === undefined || !Number.isFinite(count) || count < 1) return null;
  const whole = Math.floor(count);
  return (
    <span className="zs-badge" aria-label={`${whole} unread`}>
      {whole > 99 ? '99+' : whole}
    </span>
  );
}

function RailButton({
  label,
  children,
  selected = false,
  badge,
  onClick,
  expanded,
  controls,
}: {
  label: string;
  children: ReactNode;
  selected?: boolean;
  badge: number | undefined;
  onClick: MouseEventHandler<HTMLButtonElement>;
  expanded?: boolean;
  controls?: string;
}) {
  const tooltipId = useId();
  const [tooltipPosition, setTooltipPosition] = useState<{ top: number; left: number } | null>(
    null,
  );
  function showTooltip(element: HTMLElement) {
    const bounds = element.getBoundingClientRect();
    setTooltipPosition({ top: bounds.top + bounds.height / 2, left: bounds.right + 12 });
  }
  return (
    <span className="zs-rail-control">
      <button
        type="button"
        className="zs-icon-button"
        aria-label={
          badge !== undefined && Number.isFinite(badge) && badge >= 1
            ? `${label}, ${Math.floor(badge)} unread`
            : label
        }
        aria-current={selected ? 'page' : undefined}
        aria-describedby={tooltipId}
        aria-expanded={expanded}
        aria-controls={controls}
        onClick={onClick}
        onPointerEnter={(event) => showTooltip(event.currentTarget)}
        onPointerLeave={() => setTooltipPosition(null)}
        onFocus={(event) => showTooltip(event.currentTarget)}
        onBlur={() => setTooltipPosition(null)}
      >
        {children}
        <UnreadBadge count={badge} />
      </button>
      <span
        role="tooltip"
        id={tooltipId}
        className="zs-tooltip"
        hidden={tooltipPosition === null}
        style={tooltipPosition ?? undefined}
      >
        {label}
      </span>
    </span>
  );
}

const narrowQuery = '(max-width: 760px)';
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

function ThemeControl({ theme, onThemeChange }: Pick<ZoenShellProps, 'theme' | 'onThemeChange'>) {
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

/** An application frame. Data, navigation, search, theme, and content belong to the host. */
export function ZoenShell<NavigationId extends string, ChatId extends string>({
  navigation,
  activeNavigationId,
  onNavigate,
  searchNavigationId,
  chats,
  activeChatId,
  onSelectChat,
  searchQuery,
  onSearchChange,
  theme,
  onThemeChange,
  title = '',
  children,
  sidebarTitle = 'Chats',
  searchPlaceholder = 'Search Zoen',
  searchInputRef,
  globalSearchShortcut = false,
  headerActions,
  sidebarFilters,
  sidebarFooter,
  emptySidebar = 'No chats to show.',
  onNewChat,
  defaultCollapsed = false,
  className = '',
}: ZoenShellProps<NavigationId, ChatId>) {
  const sidebarId = useId();
  const sidebarHeadingId = useId();
  const contentId = useId();
  const searchId = useId();
  const [collapsed, setCollapsed] = useState(defaultCollapsed);
  const [narrow, setNarrow] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [focusSearch, setFocusSearch] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const sidebarRef = useRef<HTMLElement>(null);
  const toggleRef = useRef<HTMLButtonElement>(null);
  const searchFieldRef = useRef<HTMLInputElement>(null);
  const drawerReturnFocus = useRef<HTMLElement | null>(null);
  const modalOpen = narrow && drawerOpen;
  const sidebarVisible = narrow ? drawerOpen : !collapsed;

  const attachSearch = useCallback(
    (element: HTMLInputElement | null) => {
      searchFieldRef.current = element;
      if (typeof searchInputRef === 'function') {
        const cleanup = searchInputRef(element);
        if (typeof cleanup === 'function')
          return () => {
            searchFieldRef.current = null;
            cleanup();
          };
      } else if (searchInputRef) searchInputRef.current = element;
    },
    [searchInputRef],
  );

  const requestSearch = useCallback(
    (opener?: HTMLElement) => {
      if (narrow) {
        if (!drawerOpen) {
          const returnFocus = opener ?? document.activeElement;
          if (returnFocus instanceof HTMLElement) drawerReturnFocus.current = returnFocus;
        }
        setDrawerOpen(true);
      } else setCollapsed(false);
      setFocusSearch(true);
    },
    [narrow, drawerOpen],
  );

  useEffect(() => {
    const query = window.matchMedia(narrowQuery);
    const update = () => {
      setNarrow(query.matches);
      setDrawerOpen(false);
    };
    update();
    query.addEventListener('change', update);
    return () => query.removeEventListener('change', update);
  }, []);

  useEffect(() => {
    if (!modalOpen) return;
    const sidebar = sidebarRef.current;
    if (!sidebar) return;
    const previousFocus = document.activeElement;
    const focusable = () =>
      Array.from(
        sidebar.querySelectorAll<HTMLElement>(
          'button, a[href], input, select, textarea, summary, [tabindex]',
        ),
      ).filter(
        (element) =>
          element.tabIndex >= 0 &&
          !element.matches(':disabled') &&
          !element.closest('[hidden], [inert]') &&
          getComputedStyle(element).visibility !== 'hidden' &&
          element.getClientRects().length > 0,
      );
    (focusable()[0] ?? sidebar).focus();
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        setDrawerOpen(false);
        return;
      }
      if (event.key !== 'Tab') return;
      const elements = focusable();
      const first = elements[0] ?? sidebar;
      const last = elements.at(-1) ?? sidebar;
      if (
        event.shiftKey &&
        (document.activeElement === first || !sidebar.contains(document.activeElement))
      ) {
        event.preventDefault();
        last.focus();
      } else if (
        !event.shiftKey &&
        (document.activeElement === last || !sidebar.contains(document.activeElement))
      ) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('keydown', handleKey);
      const returnFocus = drawerReturnFocus.current ?? previousFocus;
      drawerReturnFocus.current = null;
      if (
        returnFocus instanceof HTMLElement &&
        returnFocus.isConnected &&
        !sidebar.contains(returnFocus) &&
        returnFocus !== document.body
      )
        returnFocus.focus();
      else if (toggleRef.current?.isConnected) toggleRef.current.focus();
    };
  }, [modalOpen]);

  useEffect(() => {
    if (!focusSearch || !sidebarVisible) return;
    searchFieldRef.current?.focus();
    searchFieldRef.current?.select();
    setFocusSearch(false);
  }, [focusSearch, sidebarVisible]);

  useEffect(() => {
    const handleFind = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.isComposing ||
        event.altKey ||
        event.shiftKey ||
        !(event.metaKey || event.ctrlKey) ||
        event.key.toLowerCase() !== 'f'
      )
        return;
      const root = rootRef.current;
      if (
        !root ||
        (!root.contains(document.activeElement) &&
          !(globalSearchShortcut && document.activeElement === document.body))
      )
        return;
      event.preventDefault();
      requestSearch();
    };
    document.addEventListener('keydown', handleFind);
    return () => document.removeEventListener('keydown', handleFind);
  }, [requestSearch, globalSearchShortcut]);

  return (
    <div
      ref={rootRef}
      className={`zs-shell ${className}`}
      data-theme={theme}
      data-collapsed={!sidebarVisible}
      data-narrow={narrow}
    >
      <a className="zs-skip-link" href={`#${contentId}`} inert={modalOpen}>
        Skip to content
      </a>
      <div className="zs-rail" inert={modalOpen}>
        <button
          ref={toggleRef}
          type="button"
          className="zs-icon-button zs-panel-toggle"
          aria-label={`${sidebarVisible ? 'Hide' : 'Show'} ${sidebarTitle.toLowerCase()}`}
          title={`${sidebarVisible ? 'Hide' : 'Show'} ${sidebarTitle.toLowerCase()}`}
          aria-expanded={sidebarVisible}
          aria-controls={sidebarId}
          onClick={(event) => {
            if (narrow) {
              if (!drawerOpen) drawerReturnFocus.current = event.currentTarget;
              setDrawerOpen((open) => !open);
            } else setCollapsed((value) => !value);
          }}
        >
          <ShellIcon name="panel" />
        </button>
        <nav className="zs-rail-nav" aria-label="Main navigation">
          {navigation.map((item) => (
            <RailButton
              key={item.id}
              label={item.label}
              selected={item.id === activeNavigationId}
              badge={item.badge}
              {...(item.id === searchNavigationId
                ? { controls: sidebarId, expanded: sidebarVisible }
                : {})}
              onClick={(event) => {
                onNavigate(item.id);
                if (item.id === searchNavigationId) requestSearch(event.currentTarget);
              }}
            >
              {item.icon}
            </RailButton>
          ))}
        </nav>
        <ThemeControl theme={theme} onThemeChange={onThemeChange} />
      </div>
      <div className="zs-frame">
        <div className="zs-body">
          <aside
            ref={sidebarRef}
            id={sidebarId}
            className="zs-sidebar"
            hidden={!sidebarVisible}
            role={modalOpen ? 'dialog' : undefined}
            aria-modal={modalOpen ? true : undefined}
            aria-labelledby={sidebarHeadingId}
            tabIndex={-1}
          >
            <div className="zs-sidebar-header">
              <h2 id={sidebarHeadingId}>{sidebarTitle}</h2>
              <div className="zs-sidebar-actions">
                {onNewChat && (
                  <button
                    type="button"
                    className="zs-icon-button"
                    aria-label="New chat"
                    title="New chat"
                    onClick={() => {
                      onNewChat();
                      if (narrow) setDrawerOpen(false);
                    }}
                  >
                    <ShellIcon name="compose" />
                  </button>
                )}
                {narrow && (
                  <button
                    type="button"
                    className="zs-icon-button"
                    aria-label={`Close ${sidebarTitle.toLowerCase()}`}
                    title={`Close ${sidebarTitle.toLowerCase()}`}
                    onClick={() => setDrawerOpen(false)}
                  >
                    <ShellIcon name="close" />
                  </button>
                )}
              </div>
            </div>
            <div className="zs-search" role="search">
              <ShellIcon name="search" />
              <label className="zs-sr-only" htmlFor={searchId}>
                {searchPlaceholder}
              </label>
              <input
                ref={attachSearch}
                id={searchId}
                type="search"
                placeholder={searchPlaceholder}
                value={searchQuery}
                onChange={(event) => onSearchChange(event.currentTarget.value)}
              />
            </div>
            {sidebarFilters && <div className="zs-sidebar-filters">{sidebarFilters}</div>}
            <nav className="zs-chat-list" aria-label={sidebarTitle}>
              {chats.length === 0 ? (
                <div className="zs-empty">{emptySidebar}</div>
              ) : (
                chats.map((chat) => (
                  <button
                    key={chat.id}
                    type="button"
                    className="zs-chat-row"
                    aria-current={chat.id === activeChatId ? 'page' : undefined}
                    onClick={() => {
                      onSelectChat(chat.id);
                      if (narrow) setDrawerOpen(false);
                    }}
                  >
                    <span className="zs-chat-avatar" aria-hidden="true">
                      {chat.avatar ?? chat.title.slice(0, 1)}
                    </span>
                    <span className="zs-chat-copy">
                      <span className="zs-chat-title">{chat.title}</span>
                      {chat.preview && <span className="zs-chat-preview">{chat.preview}</span>}
                    </span>
                    <span className="zs-chat-meta">
                      {chat.time && <span className="zs-chat-time">{chat.time}</span>}
                      <UnreadBadge count={chat.unreadCount} />
                    </span>
                  </button>
                ))
              )}
            </nav>
            {sidebarFooter && <div className="zs-sidebar-footer">{sidebarFooter}</div>}
          </aside>
          <main id={contentId} className="zs-content" tabIndex={-1} inert={modalOpen}>
            {(title || headerActions) && (
              <div className="zs-content-context">
                {title && (
                  <span className="zs-context" title={title}>
                    {title}
                  </span>
                )}
                {headerActions && <div className="zs-context-actions">{headerActions}</div>}
              </div>
            )}
            <div className="zs-content-body">{children}</div>
          </main>
        </div>
      </div>
      {modalOpen && (
        <button
          type="button"
          tabIndex={-1}
          className="zs-drawer-backdrop"
          aria-label={`Close ${sidebarTitle.toLowerCase()}`}
          onClick={() => setDrawerOpen(false)}
        />
      )}
    </div>
  );
}
